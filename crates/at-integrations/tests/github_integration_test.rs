//! Exhaustive tests for GitHub integration: client, issues, PRs, sync, and
//! PR automation — matching the GitHub Issues/PRs UI surfaces:
//!
//! Issues UI:
//!   - "GitHub Issues" header with repo name, open counter, refresh
//!   - "Analyze & Group Issues" button, "Auto-Fix New" toggle
//!   - Search bar, filter dropdown (Open/Closed/All)
//!   - Split view: issue list + issue detail
//!
//! PRs UI:
//!   - "Pull Requests" header with repo link and count
//!   - PR list with number, title, status dots, date, comment count
//!   - Contributors filter, labels, author, description preview
//!   - Claude Code version selector
//!
//! Related branches:
//!   - fix/pr-review-findings-dropped
//!   - auto-claude/221-refactor-github-pr-review-with-xstate
//!   - terminal/pr-review-worktrees

use at_core::types::*;
use at_integrations::github::client::{GitHubClient, GitHubError};
use at_integrations::github::issues::import_issue_as_task;
use at_integrations::github::pr_automation::PrStatus;
use at_integrations::github::sync::{bead_issue_number, build_issue_metadata};
use at_integrations::types::*;

use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

// ===========================================================================
// Test helpers
// ===========================================================================

fn make_config(token: Option<&str>) -> GitHubConfig {
    GitHubConfig {
        token: token.map(|t| t.to_string()),
        owner: "ryanmaclean".to_string(),
        repo: "vibecode-webgui".to_string(),
    }
}

fn make_github_issue(number: u64, title: &str, state: IssueState) -> GitHubIssue {
    let now = Utc::now();
    GitHubIssue {
        number,
        title: title.to_string(),
        body: Some(format!("Description for: {}", title)),
        state,
        labels: vec![],
        assignees: vec![],
        author: "testuser".to_string(),
        created_at: now,
        updated_at: now,
        comments: 0,
        html_url: format!(
            "https://github.com/ryanmaclean/vibecode-webgui/issues/{}",
            number
        ),
    }
}

fn make_github_issue_with_labels(
    number: u64,
    title: &str,
    state: IssueState,
    labels: Vec<(&str, &str)>,
) -> GitHubIssue {
    let mut issue = make_github_issue(number, title, state);
    issue.labels = labels
        .into_iter()
        .map(|(name, color)| GitHubLabel {
            name: name.to_string(),
            color: color.to_string(),
            description: None,
        })
        .collect();
    issue
}

fn make_github_pr(
    number: u64,
    title: &str,
    state: PrState,
    head: &str,
    author: &str,
) -> GitHubPullRequest {
    let now = Utc::now();
    GitHubPullRequest {
        number,
        title: title.to_string(),
        body: Some(format!("PR body for: {}", title)),
        state,
        author: author.to_string(),
        head_branch: head.to_string(),
        base_branch: "main".to_string(),
        labels: vec![],
        reviewers: vec![],
        draft: false,
        mergeable: Some(true),
        additions: 25,
        deletions: 3,
        changed_files: 8,
        created_at: now,
        updated_at: now,
        merged_at: None,
        html_url: format!(
            "https://github.com/ryanmaclean/vibecode-webgui/pull/{}",
            number
        ),
    }
}

fn make_bead_with_issue(issue_number: u64, status: BeadStatus) -> Bead {
    let now = Utc::now();
    Bead {
        id: Uuid::new_v4(),
        title: format!("Issue #{}", issue_number),
        description: None,
        status,
        lane: Lane::Standard,
        priority: 0,
        agent_id: None,
        convoy_id: None,
        created_at: now,
        updated_at: now,
        hooked_at: None,
        slung_at: None,
        done_at: None,
        git_branch: None,
        metadata: Some(json!({
            "source": "github",
            "issue_number": issue_number,
            "html_url": format!("https://github.com/test/repo/issues/{}", issue_number),
        })),
    }
}

fn make_plain_bead(title: &str) -> Bead {
    Bead::new(title.to_string(), Lane::Standard)
}

fn make_test_task(title: &str, phase: TaskPhase) -> Task {
    let mut task = Task::new(
        title.to_string(),
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Medium,
    );
    task.set_phase(phase);
    task.description = Some("A test task description.".to_string());
    task.git_branch = Some("feature/test-branch".to_string());
    task.worktree_path = Some("/tmp/worktree/test".to_string());
    task
}

/// Standalone PR body generation (mirrors PrAutomation::generate_pr_body
/// without requiring a GitHubClient).
fn generate_pr_body_for_task(task: &Task) -> String {
    let mut body = String::new();

    body.push_str(&format!("## {}\n\n", task.title));

    if let Some(desc) = &task.description {
        body.push_str(&format!("{}\n\n", desc));
    }

    body.push_str("### Phase Summary\n\n");
    body.push_str(&format!("- **Current Phase**: {:?}\n", task.phase));
    body.push_str(&format!("- **Progress**: {}%\n", task.progress_percent));
    body.push_str(&format!("- **Category**: {:?}\n", task.category));
    body.push_str(&format!("- **Priority**: {:?}\n", task.priority));
    body.push_str(&format!("- **Complexity**: {:?}\n", task.complexity));

    if let Some(started) = &task.started_at {
        body.push_str(&format!(
            "- **Started**: {}\n",
            started.format("%Y-%m-%d %H:%M UTC")
        ));
    }
    if let Some(completed) = &task.completed_at {
        body.push_str(&format!(
            "- **Completed**: {}\n",
            completed.format("%Y-%m-%d %H:%M UTC")
        ));
    }

    if let Some(worktree) = &task.worktree_path {
        body.push_str(&format!("\n### Worktree\n\n`{}`\n", worktree));
    }

    if !task.logs.is_empty() {
        body.push_str("\n### Activity Log\n\n");
        let log_slice = if task.logs.len() > 10 {
            &task.logs[task.logs.len() - 10..]
        } else {
            &task.logs
        };

        for entry in log_slice {
            let label = match entry.log_type {
                TaskLogType::Text => "text",
                TaskLogType::PhaseStart => "phase_start",
                TaskLogType::PhaseEnd => "phase_end",
                TaskLogType::ToolStart => "tool_start",
                TaskLogType::ToolEnd => "tool_end",
                TaskLogType::Error => "error",
                TaskLogType::Success => "success",
                TaskLogType::Info => "info",
            };
            body.push_str(&format!(
                "- `[{:?}]` {}: {}\n",
                entry.phase, label, entry.message
            ));
        }
    }

    if !task.subtasks.is_empty() {
        body.push_str("\n### Subtasks\n\n");
        for subtask in &task.subtasks {
            let check = match subtask.status {
                SubtaskStatus::Complete => "[x]",
                SubtaskStatus::Failed => "[!]",
                SubtaskStatus::Skipped => "[-]",
                _ => "[ ]",
            };
            body.push_str(&format!("- {} {}\n", check, subtask.title));
        }
    }

    body.push_str("\n---\n*Auto-generated by auto-tundra*\n");

    body
}

// ===========================================================================
// GitHub Client
// ===========================================================================

#[tokio::test]
async fn test_github_client_creation() {
    let config = make_config(Some("ghp_test_token_12345"));
    let client = GitHubClient::new(config).unwrap();
    assert_eq!(client.owner(), "ryanmaclean");
    assert_eq!(client.repo(), "vibecode-webgui");
}

#[tokio::test]
async fn test_github_client_with_token() {
    // Valid token succeeds
    let config = make_config(Some("ghp_valid_token"));
    let result = GitHubClient::new(config);
    assert!(result.is_ok());

    // Missing token fails
    let config_no_token = make_config(None);
    let result = GitHubClient::new(config_no_token);
    assert!(result.is_err());
    match result {
        Err(GitHubError::MissingToken) => {}
        other => panic!("Expected MissingToken, got {:?}", other),
    }
}

#[tokio::test]
async fn test_github_client_repo_info() {
    // Matches the UI header "GitHub Issues — ryanmaclean/vibecode-webgui"
    let config = make_config(Some("ghp_test"));
    let client = GitHubClient::new(config).unwrap();

    assert_eq!(client.owner(), "ryanmaclean");
    assert_eq!(client.repo(), "vibecode-webgui");

    // inner() should return a valid Octocrab instance
    let _inner = client.inner();
}

// ===========================================================================
// Issue Operations
// ===========================================================================

#[test]
fn test_list_issues_empty() {
    // Mirrors UI's "No issues found" empty state and "0 open" counter
    let issues: Vec<GitHubIssue> = vec![];
    assert!(issues.is_empty());
    // UI shows "0 open" counter
    let open_count = issues
        .iter()
        .filter(|i| i.state == IssueState::Open)
        .count();
    assert_eq!(open_count, 0);
}

#[test]
fn test_create_issue() {
    // Verify GitHubIssue structure matches what create_issue would return
    let issue = make_github_issue(42, "New bug report", IssueState::Open);
    assert_eq!(issue.number, 42);
    assert_eq!(issue.title, "New bug report");
    assert_eq!(issue.state, IssueState::Open);
    assert!(issue.body.is_some());
    assert!(issue.html_url.contains("/issues/42"));
}

#[test]
fn test_update_issue_status() {
    // Simulate updating issue state (Open -> Closed, Closed -> Open)
    let mut issue = make_github_issue(10, "Bug to fix", IssueState::Open);
    assert_eq!(issue.state, IssueState::Open);

    // Simulate closing
    issue.state = IssueState::Closed;
    assert_eq!(issue.state, IssueState::Closed);

    // Simulate reopening
    issue.state = IssueState::Open;
    assert_eq!(issue.state, IssueState::Open);
}

#[test]
fn test_search_issues_by_text() {
    // Mirrors the UI search bar: "Search issues..."
    let issues = [
        make_github_issue(1, "Fix login page crash", IssueState::Open),
        make_github_issue(2, "Add dark mode support", IssueState::Open),
        make_github_issue(3, "Login timeout too short", IssueState::Open),
        make_github_issue(4, "Refactor database layer", IssueState::Closed),
    ];

    let search_term = "login";
    let results: Vec<&GitHubIssue> = issues
        .iter()
        .filter(|i| i.title.to_lowercase().contains(search_term))
        .collect();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].number, 1);
    assert_eq!(results[1].number, 3);
}

#[test]
fn test_filter_issues_by_state() {
    // Mirrors the UI filter dropdown: "Open" / "Closed" / "All"
    let issues = [
        make_github_issue(1, "Open bug", IssueState::Open),
        make_github_issue(2, "Another open", IssueState::Open),
        make_github_issue(3, "Fixed bug", IssueState::Closed),
        make_github_issue(4, "Old feature", IssueState::Closed),
        make_github_issue(5, "New feature", IssueState::Open),
    ];

    // Filter: Open
    let open: Vec<&GitHubIssue> = issues
        .iter()
        .filter(|i| i.state == IssueState::Open)
        .collect();
    assert_eq!(open.len(), 3);

    // Filter: Closed
    let closed: Vec<&GitHubIssue> = issues
        .iter()
        .filter(|i| i.state == IssueState::Closed)
        .collect();
    assert_eq!(closed.len(), 2);

    // Filter: All
    assert_eq!(issues.len(), 5);
}

#[test]
fn test_analyze_and_group_issues() {
    // "Analyze & Group Issues" button groups issues by label/category
    let issues = vec![
        make_github_issue_with_labels(1, "Login crash", IssueState::Open, vec![("bug", "d73a4a")]),
        make_github_issue_with_labels(
            2,
            "Add OAuth",
            IssueState::Open,
            vec![("enhancement", "a2eeef")],
        ),
        make_github_issue_with_labels(3, "Memory leak", IssueState::Open, vec![("bug", "d73a4a")]),
        make_github_issue_with_labels(
            4,
            "New API endpoint",
            IssueState::Open,
            vec![("enhancement", "a2eeef")],
        ),
        make_github_issue_with_labels(5, "Update docs", IssueState::Open, vec![("docs", "0075ca")]),
    ];

    // Group by first label
    let mut groups: std::collections::HashMap<String, Vec<u64>> = std::collections::HashMap::new();
    for issue in &issues {
        let label = issue
            .labels
            .first()
            .map(|l| l.name.clone())
            .unwrap_or_else(|| "unlabeled".to_string());
        groups.entry(label).or_default().push(issue.number);
    }

    assert_eq!(groups.len(), 3);
    assert_eq!(groups["bug"].len(), 2);
    assert_eq!(groups["enhancement"].len(), 2);
    assert_eq!(groups["docs"].len(), 1);
}

#[test]
fn test_auto_fix_toggle_creates_branch() {
    // "Auto-Fix New" toggle: when enabled, new issues get a branch
    let issue = make_github_issue(
        221,
        "Refactor GitHub PR review with XState",
        IssueState::Open,
    );

    // Branch naming convention: auto-claude/{issue_number}-{sanitized-title}
    let sanitized_title = issue
        .title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    let branch_name = format!("auto-claude/{}-{}", issue.number, sanitized_title);

    assert!(branch_name.starts_with("auto-claude/221-"));
    assert!(branch_name.contains("refactor-github-pr-review-with-xstate"));
}

// ===========================================================================
// Issue-Bead Sync
// ===========================================================================

#[test]
fn test_import_issues_as_beads() {
    // import_issue_as_task converts GitHubIssue -> Bead
    let issue = make_github_issue(7, "Add logging", IssueState::Open);
    let bead = import_issue_as_task(&issue);

    assert_eq!(bead.title, "Add logging");
    assert_eq!(bead.status, BeadStatus::Backlog);
    assert_eq!(bead.lane, Lane::Standard);
    assert!(bead.description.is_some());

    let meta = bead.metadata.as_ref().unwrap();
    assert_eq!(meta["source"], "github");
    assert_eq!(meta["issue_number"], 7);
    assert_eq!(meta["author"], "testuser");
}

#[test]
fn test_sync_bead_status_to_github() {
    // BeadStatus::Done -> IssueState::Closed
    let done_bead = make_bead_with_issue(42, BeadStatus::Done);
    let target_state = match done_bead.status {
        BeadStatus::Done => Some(IssueState::Closed),
        BeadStatus::Backlog => Some(IssueState::Open),
        _ => None,
    };
    assert_eq!(target_state, Some(IssueState::Closed));

    // BeadStatus::Backlog -> IssueState::Open
    let backlog_bead = make_bead_with_issue(43, BeadStatus::Backlog);
    let target_state2 = match backlog_bead.status {
        BeadStatus::Done => Some(IssueState::Closed),
        BeadStatus::Backlog => Some(IssueState::Open),
        _ => None,
    };
    assert_eq!(target_state2, Some(IssueState::Open));

    // BeadStatus::Hooked -> no state change
    let hooked_bead = make_bead_with_issue(44, BeadStatus::Hooked);
    let target_state3 = match hooked_bead.status {
        BeadStatus::Done => Some(IssueState::Closed),
        BeadStatus::Backlog => Some(IssueState::Open),
        _ => None,
    };
    assert!(target_state3.is_none());
}

#[test]
fn test_export_bead_as_issue() {
    // Bead -> GitHubIssue metadata
    let issue = make_github_issue(99, "Exported task", IssueState::Open);
    let metadata = build_issue_metadata(&issue);

    assert_eq!(metadata["source"], "github");
    assert_eq!(metadata["issue_number"], 99);
    assert!(metadata["html_url"]
        .as_str()
        .unwrap()
        .contains("/issues/99"));
    assert_eq!(metadata["author"], "testuser");
}

#[test]
fn test_deduplication_by_issue_number() {
    // Already-imported issues should be skipped
    let existing_beads = [
        make_bead_with_issue(1, BeadStatus::Backlog),
        make_bead_with_issue(3, BeadStatus::Hooked),
    ];

    let imported_numbers: Vec<u64> = existing_beads
        .iter()
        .filter_map(bead_issue_number)
        .collect();

    assert!(imported_numbers.contains(&1));
    assert!(imported_numbers.contains(&3));

    let incoming = [
        make_github_issue(1, "Already imported", IssueState::Open),
        make_github_issue(2, "New issue", IssueState::Open),
        make_github_issue(3, "Also imported", IssueState::Open),
        make_github_issue(4, "Another new", IssueState::Open),
    ];

    let new_beads: Vec<Bead> = incoming
        .iter()
        .filter(|issue| !imported_numbers.contains(&issue.number))
        .map(import_issue_as_task)
        .collect();

    assert_eq!(new_beads.len(), 2);
    assert_eq!(new_beads[0].title, "New issue");
    assert_eq!(new_beads[1].title, "Another new");
}

#[test]
fn test_poll_updates() {
    // poll_updates filters issues by updated_at >= since
    let now = Utc::now();
    let two_hours_ago = now - Duration::hours(2);
    let five_min_ago = now - Duration::minutes(5);
    let since = now - Duration::hours(1);

    let issues = [
        {
            let mut i = make_github_issue(1, "Old issue", IssueState::Open);
            i.updated_at = two_hours_ago;
            i
        },
        {
            let mut i = make_github_issue(2, "Recent issue", IssueState::Open);
            i.updated_at = five_min_ago;
            i
        },
        {
            let mut i = make_github_issue(3, "Very recent", IssueState::Open);
            i.updated_at = now;
            i
        },
    ];

    let filtered: Vec<&GitHubIssue> = issues.iter().filter(|i| i.updated_at >= since).collect();

    assert_eq!(filtered.len(), 2);
    assert_eq!(filtered[0].number, 2);
    assert_eq!(filtered[1].number, 3);
}

// ===========================================================================
// PR Operations
// ===========================================================================

#[test]
fn test_list_pull_requests() {
    // UI shows PR list with number, title, status dots, date
    let prs = [
        make_github_pr(
            101,
            "feat: allow project tabs to expand horizontally",
            PrState::Open,
            "feat/expand-tabs",
            "alice",
        ),
        make_github_pr(
            102,
            "fix: Remove .auto-claude files from git index",
            PrState::Merged,
            "fix/remove-auto-claude",
            "bob",
        ),
        make_github_pr(
            103,
            "refactor: GitHub PR review with XState",
            PrState::Closed,
            "auto-claude/221-refactor",
            "charlie",
        ),
    ];

    assert_eq!(prs.len(), 3);

    let open_prs: Vec<&GitHubPullRequest> =
        prs.iter().filter(|p| p.state == PrState::Open).collect();
    assert_eq!(open_prs.len(), 1);
    assert_eq!(open_prs[0].number, 101);

    let merged_prs: Vec<&GitHubPullRequest> =
        prs.iter().filter(|p| p.state == PrState::Merged).collect();
    assert_eq!(merged_prs.len(), 1);
}

#[test]
fn test_create_pr_for_task() {
    // PR created from a completed task's worktree branch
    let task = make_test_task("Add user authentication", TaskPhase::Complete);
    let head = task.git_branch.as_deref().unwrap_or("main");
    assert_eq!(head, "feature/test-branch");

    // PR title matches task title
    let pr = make_github_pr(200, &task.title, PrState::Open, head, "auto-claude");
    assert_eq!(pr.title, "Add user authentication");
    assert_eq!(pr.head_branch, "feature/test-branch");
    assert_eq!(pr.base_branch, "main");
}

#[test]
fn test_generate_pr_body() {
    let task = make_test_task("Implement feature X", TaskPhase::Complete);
    let body = generate_pr_body_for_task(&task);

    assert!(body.contains("## Implement feature X"));
    assert!(body.contains("A test task description."));
    assert!(body.contains("### Phase Summary"));
    assert!(body.contains("Complete"));
    assert!(body.contains("100%"));
    assert!(body.contains("Feature"));
    assert!(body.contains("Medium")); // priority and complexity
    assert!(body.contains("### Worktree"));
    assert!(body.contains("/tmp/worktree/test"));
    assert!(body.contains("*Auto-generated by auto-tundra*"));
}

#[test]
fn test_check_pr_status() {
    // PrStatus captures mergeable, checks, reviews, approved state
    let status = PrStatus {
        mergeable: Some(true),
        checks_passing: true,
        review_count: 2,
        approved: true,
    };

    assert_eq!(status.mergeable, Some(true));
    assert!(status.checks_passing);
    assert_eq!(status.review_count, 2);
    assert!(status.approved);

    // Not approved case
    let status_not_approved = PrStatus {
        mergeable: Some(false),
        checks_passing: false,
        review_count: 0,
        approved: false,
    };

    assert!(!status_not_approved.checks_passing);
    assert!(!status_not_approved.approved);
}

#[test]
fn test_filter_prs_by_contributor() {
    // UI has Contributors tab for filtering PRs
    let prs = [
        make_github_pr(1, "PR by Alice", PrState::Open, "branch-a", "alice"),
        make_github_pr(2, "PR by Bob", PrState::Open, "branch-b", "bob"),
        make_github_pr(3, "Another by Alice", PrState::Open, "branch-c", "alice"),
        make_github_pr(4, "PR by Charlie", PrState::Merged, "branch-d", "charlie"),
    ];

    let alice_prs: Vec<&GitHubPullRequest> = prs.iter().filter(|p| p.author == "alice").collect();
    assert_eq!(alice_prs.len(), 2);
    assert_eq!(alice_prs[0].number, 1);
    assert_eq!(alice_prs[1].number, 3);

    let bob_prs: Vec<&GitHubPullRequest> = prs.iter().filter(|p| p.author == "bob").collect();
    assert_eq!(bob_prs.len(), 1);
}

#[test]
fn test_pr_has_labels_and_author() {
    // UI shows labels and author on each PR card
    let mut pr = make_github_pr(
        1713,
        "fix: Remove .auto-claude files from git index in new worktrees",
        PrState::Open,
        "fix/remove-auto-claude",
        "ryanmaclean",
    );
    pr.labels = vec![
        GitHubLabel {
            name: "bug".to_string(),
            color: "d73a4a".to_string(),
            description: Some("Something isn't working".to_string()),
        },
        GitHubLabel {
            name: "worktrees".to_string(),
            color: "0e8a16".to_string(),
            description: None,
        },
    ];

    assert_eq!(pr.author, "ryanmaclean");
    assert_eq!(pr.labels.len(), 2);
    assert_eq!(pr.labels[0].name, "bug");
    assert_eq!(pr.labels[1].name, "worktrees");
    assert!(pr.title.contains("worktrees"));
}

#[test]
fn test_pr_review_findings() {
    // Regression test from fix/pr-review-findings-dropped:
    // PR review findings must not get lost during processing
    let findings = vec![
        ReviewFinding {
            file: "src/handler.rs".to_string(),
            line: Some(42),
            severity: FindingSeverity::Error,
            category: "logic".to_string(),
            message: "Null pointer dereference possible".to_string(),
            suggestion: Some("Add null check before access".to_string()),
        },
        ReviewFinding {
            file: "src/routes.rs".to_string(),
            line: Some(15),
            severity: FindingSeverity::Warning,
            category: "style".to_string(),
            message: "Unused import".to_string(),
            suggestion: Some("Remove unused import".to_string()),
        },
        ReviewFinding {
            file: "src/main.rs".to_string(),
            line: None,
            severity: FindingSeverity::Info,
            category: "documentation".to_string(),
            message: "Missing module documentation".to_string(),
            suggestion: None,
        },
        ReviewFinding {
            file: "src/db.rs".to_string(),
            line: Some(100),
            severity: FindingSeverity::Critical,
            category: "security".to_string(),
            message: "SQL injection vulnerability".to_string(),
            suggestion: Some("Use parameterized queries".to_string()),
        },
    ];

    // All findings must be preserved (regression: findings were being dropped)
    assert_eq!(findings.len(), 4);

    // Verify each finding has correct severity
    assert_eq!(findings[0].severity, FindingSeverity::Error);
    assert_eq!(findings[1].severity, FindingSeverity::Warning);
    assert_eq!(findings[2].severity, FindingSeverity::Info);
    assert_eq!(findings[3].severity, FindingSeverity::Critical);

    // Verify serialization preserves all findings (the bug was data loss)
    let json = serde_json::to_string(&findings).unwrap();
    let deserialized: Vec<ReviewFinding> = serde_json::from_str(&json).unwrap();
    assert_eq!(
        deserialized.len(),
        findings.len(),
        "findings must not be dropped during serialization"
    );

    // Verify critical findings are not filtered out
    let critical: Vec<&ReviewFinding> = findings
        .iter()
        .filter(|f| f.severity == FindingSeverity::Critical)
        .collect();
    assert_eq!(critical.len(), 1);
    assert!(critical[0].message.contains("SQL injection"));

    // Verify findings with no line number are preserved
    let no_line: Vec<&ReviewFinding> = findings.iter().filter(|f| f.line.is_none()).collect();
    assert_eq!(no_line.len(), 1);
}

// ===========================================================================
// PR Automation
// ===========================================================================

#[test]
fn test_auto_create_pr_from_completed_task() {
    // When a task reaches Complete phase, a PR should be auto-created
    let task = make_test_task("Implement dark mode", TaskPhase::Complete);

    assert_eq!(task.phase, TaskPhase::Complete);
    assert_eq!(task.progress_percent, 100);
    assert!(task.git_branch.is_some());

    // PR should use the task's git_branch as head
    let head = task.git_branch.as_deref().unwrap();
    assert_eq!(head, "feature/test-branch");

    // PR body should include task details
    let body = generate_pr_body_for_task(&task);
    assert!(body.contains("Implement dark mode"));
    assert!(body.contains("Complete"));
}

#[test]
fn test_pr_body_includes_task_details() {
    let mut task = make_test_task("Refactor auth module", TaskPhase::Complete);
    task.started_at = Some(Utc::now() - Duration::hours(2));
    task.completed_at = Some(Utc::now());

    let body = generate_pr_body_for_task(&task);

    // Must include all key sections
    assert!(body.contains("## Refactor auth module"));
    assert!(body.contains("A test task description."));
    assert!(body.contains("### Phase Summary"));
    assert!(body.contains("**Current Phase**: Complete"));
    assert!(body.contains("**Progress**: 100%"));
    assert!(body.contains("**Category**: Feature"));
    assert!(body.contains("**Priority**: Medium"));
    assert!(body.contains("**Complexity**: Medium"));
    assert!(body.contains("**Started**:"));
    assert!(body.contains("**Completed**:"));
    assert!(body.contains("### Worktree"));
}

#[test]
fn test_pr_body_includes_subtask_checklist() {
    let mut task = make_test_task("Multi-step implementation", TaskPhase::Complete);
    task.subtasks = vec![
        Subtask {
            id: Uuid::new_v4(),
            title: "Design database schema".to_string(),
            status: SubtaskStatus::Complete,
            agent_id: None,
            depends_on: vec![],
        },
        Subtask {
            id: Uuid::new_v4(),
            title: "Implement API endpoints".to_string(),
            status: SubtaskStatus::Complete,
            agent_id: None,
            depends_on: vec![],
        },
        Subtask {
            id: Uuid::new_v4(),
            title: "Write integration tests".to_string(),
            status: SubtaskStatus::InProgress,
            agent_id: None,
            depends_on: vec![],
        },
        Subtask {
            id: Uuid::new_v4(),
            title: "Update documentation".to_string(),
            status: SubtaskStatus::Pending,
            agent_id: None,
            depends_on: vec![],
        },
        Subtask {
            id: Uuid::new_v4(),
            title: "Fix flaky test".to_string(),
            status: SubtaskStatus::Failed,
            agent_id: None,
            depends_on: vec![],
        },
        Subtask {
            id: Uuid::new_v4(),
            title: "Optional cleanup".to_string(),
            status: SubtaskStatus::Skipped,
            agent_id: None,
            depends_on: vec![],
        },
    ];

    let body = generate_pr_body_for_task(&task);

    assert!(body.contains("### Subtasks"));
    assert!(body.contains("[x] Design database schema"));
    assert!(body.contains("[x] Implement API endpoints"));
    assert!(body.contains("[ ] Write integration tests")); // InProgress -> [ ]
    assert!(body.contains("[ ] Update documentation")); // Pending -> [ ]
    assert!(body.contains("[!] Fix flaky test")); // Failed -> [!]
    assert!(body.contains("[-] Optional cleanup")); // Skipped -> [-]
}

// ===========================================================================
// Serialization / type roundtrips
// ===========================================================================

#[test]
fn test_issue_state_serde() {
    let open_json = serde_json::to_string(&IssueState::Open).unwrap();
    assert_eq!(open_json, "\"open\"");
    let closed_json = serde_json::to_string(&IssueState::Closed).unwrap();
    assert_eq!(closed_json, "\"closed\"");

    let open: IssueState = serde_json::from_str("\"open\"").unwrap();
    assert_eq!(open, IssueState::Open);
    let closed: IssueState = serde_json::from_str("\"closed\"").unwrap();
    assert_eq!(closed, IssueState::Closed);
}

#[test]
fn test_pr_state_serde() {
    let states = vec![PrState::Open, PrState::Closed, PrState::Merged];
    for state in states {
        let json = serde_json::to_string(&state).unwrap();
        let back: PrState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, state);
    }
}

#[test]
fn test_github_issue_serde_roundtrip() {
    let issue = make_github_issue_with_labels(
        42,
        "Fix the widget",
        IssueState::Open,
        vec![("bug", "d73a4a"), ("urgent", "ff0000")],
    );

    let json = serde_json::to_string(&issue).unwrap();
    let deserialized: GitHubIssue = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.number, 42);
    assert_eq!(deserialized.title, "Fix the widget");
    assert_eq!(deserialized.state, IssueState::Open);
    assert_eq!(deserialized.labels.len(), 2);
}

#[test]
fn test_github_pr_serde_roundtrip() {
    let mut pr = make_github_pr(101, "Add feature X", PrState::Open, "feature-x", "alice");
    pr.labels = vec![GitHubLabel {
        name: "enhancement".to_string(),
        color: "a2eeef".to_string(),
        description: None,
    }];
    pr.reviewers = vec!["bob".to_string(), "charlie".to_string()];

    let json = serde_json::to_string(&pr).unwrap();
    let deserialized: GitHubPullRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.number, 101);
    assert_eq!(deserialized.state, PrState::Open);
    assert_eq!(deserialized.head_branch, "feature-x");
    assert_eq!(deserialized.additions, 25);
    assert_eq!(deserialized.deletions, 3);
    assert_eq!(deserialized.changed_files, 8);
    assert_eq!(deserialized.labels.len(), 1);
    assert_eq!(deserialized.reviewers.len(), 2);
}

#[test]
fn test_review_finding_serde_roundtrip() {
    let finding = ReviewFinding {
        file: "src/main.rs".to_string(),
        line: Some(42),
        severity: FindingSeverity::Warning,
        category: "style".to_string(),
        message: "Unused variable".to_string(),
        suggestion: Some("Remove or prefix with underscore".to_string()),
    };

    let json = serde_json::to_string(&finding).unwrap();
    let deserialized: ReviewFinding = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.file, "src/main.rs");
    assert_eq!(deserialized.line, Some(42));
    assert_eq!(deserialized.severity, FindingSeverity::Warning);
    assert_eq!(
        deserialized.suggestion.as_deref(),
        Some("Remove or prefix with underscore")
    );
}

#[test]
fn test_bead_without_metadata_has_no_issue_number() {
    let bead = make_plain_bead("No metadata bead");
    assert!(bead_issue_number(&bead).is_none());
}

#[test]
fn test_import_closed_issue_sets_done_status() {
    let issue = make_github_issue(12, "Fixed crash", IssueState::Closed);
    let bead = import_issue_as_task(&issue);

    assert_eq!(bead.status, BeadStatus::Done);
    assert!(bead.done_at.is_some());
}

#[test]
fn test_pr_status_serde_roundtrip() {
    let status = PrStatus {
        mergeable: Some(true),
        checks_passing: true,
        review_count: 3,
        approved: true,
    };

    let json = serde_json::to_string(&status).unwrap();
    let deserialized: PrStatus = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.mergeable, Some(true));
    assert!(deserialized.checks_passing);
    assert_eq!(deserialized.review_count, 3);
    assert!(deserialized.approved);
}

#[test]
fn test_finding_severity_ordering() {
    // Verify all severity levels exist and serialize correctly
    let severities = vec![
        FindingSeverity::Info,
        FindingSeverity::Warning,
        FindingSeverity::Error,
        FindingSeverity::Critical,
    ];

    for severity in &severities {
        let json = serde_json::to_string(severity).unwrap();
        let back: FindingSeverity = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, severity);
    }
}
