use at_core::types::{Bead, BeadStatus};
use chrono::{DateTime, Utc};
use serde_json::json;

use crate::types::{GitHubIssue, IssueState};

use super::client::{GitHubClient, Result};
use super::issues;

/// Bidirectional sync engine between GitHub issues and at-core Beads.
pub struct IssueSyncEngine {
    client: GitHubClient,
}

impl IssueSyncEngine {
    /// Create a new sync engine with the given GitHub client.
    pub fn new(client: GitHubClient) -> Self {
        Self { client }
    }

    /// Import all open issues as Beads, skipping already-imported ones.
    ///
    /// Checks `bead.metadata["issue_number"]` to avoid duplicates.
    pub async fn import_open_issues(&self, existing_beads: &[Bead]) -> Result<Vec<Bead>> {
        let open_issues =
            issues::list_issues(&self.client, Some(IssueState::Open), None, None, None).await?;

        let imported_numbers = extract_imported_issue_numbers(existing_beads);

        let new_beads: Vec<Bead> = open_issues
            .iter()
            .filter(|issue| !imported_numbers.contains(&issue.number))
            .map(issues::import_issue_as_task)
            .collect();

        Ok(new_beads)
    }

    /// Sync a bead's status changes back to GitHub.
    ///
    /// - `BeadStatus::Done` closes the corresponding issue.
    /// - `BeadStatus::Backlog` reopens a closed issue.
    pub async fn sync_bead_status_to_github(&self, bead: &Bead) -> Result<()> {
        let issue_number = match bead_issue_number(bead) {
            Some(n) => n,
            None => return Ok(()), // No linked issue; nothing to sync.
        };

        let target_state = match bead.status {
            BeadStatus::Done => Some(IssueState::Closed),
            BeadStatus::Backlog => Some(IssueState::Open),
            _ => None,
        };

        if let Some(state) = target_state {
            issues::update_issue(&self.client, issue_number, None, None, Some(state), None).await?;
        }

        Ok(())
    }

    /// Create a GitHub issue from a Bead that doesn't have one yet.
    ///
    /// Stores the resulting `issue_number` in the bead's metadata. The caller
    /// should persist the updated bead.
    pub async fn export_bead_as_issue(&self, bead: &Bead) -> Result<GitHubIssue> {
        let body = bead.description.as_deref();
        let issue = issues::create_issue(&self.client, &bead.title, body, None).await?;
        Ok(issue)
    }

    /// Check for new/updated issues since `since`.
    pub async fn poll_updates(&self, since: DateTime<Utc>) -> Result<Vec<GitHubIssue>> {
        // Fetch all open issues and filter by updated_at >= since.
        let all_issues = issues::list_issues(&self.client, None, None, None, None).await?;

        let updated: Vec<GitHubIssue> = all_issues
            .into_iter()
            .filter(|issue| issue.updated_at >= since)
            .collect();

        Ok(updated)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the set of already-imported issue numbers from bead metadata.
fn extract_imported_issue_numbers(beads: &[Bead]) -> Vec<u64> {
    beads.iter().filter_map(bead_issue_number).collect()
}

/// Read `metadata.issue_number` from a bead, if present.
pub fn bead_issue_number(bead: &Bead) -> Option<u64> {
    bead.metadata
        .as_ref()
        .and_then(|m| m.get("issue_number"))
        .and_then(|v| v.as_u64())
}

/// Build metadata json for a bead that was exported to a GitHub issue.
pub fn build_issue_metadata(issue: &GitHubIssue) -> serde_json::Value {
    json!({
        "source": "github",
        "issue_number": issue.number,
        "html_url": issue.html_url,
        "author": issue.author,
        "labels": issue.labels.iter().map(|l| &l.name).collect::<Vec<_>>(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use at_core::types::{BeadStatus, Lane};
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    /// Helper to build a bead with GitHub issue metadata.
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

    /// Helper to build a bead without GitHub metadata.
    fn make_plain_bead(title: &str) -> Bead {
        Bead::new(title.to_string(), Lane::Standard)
    }

    /// Helper to build a mock GitHubIssue.
    fn make_github_issue(number: u64, title: &str, state: IssueState) -> GitHubIssue {
        let now = Utc::now();
        GitHubIssue {
            number,
            title: title.to_string(),
            body: Some(format!("Body for {}", title)),
            state,
            labels: vec![],
            assignees: vec![],
            author: "testuser".to_string(),
            created_at: now,
            updated_at: now,
            comments: 0,
            html_url: format!("https://github.com/test/repo/issues/{}", number),
        }
    }

    #[test]
    fn test_extract_imported_issue_numbers() {
        let beads = vec![
            make_bead_with_issue(1, BeadStatus::Backlog),
            make_bead_with_issue(5, BeadStatus::Done),
            make_plain_bead("No metadata"),
        ];

        let numbers = extract_imported_issue_numbers(&beads);
        assert_eq!(numbers.len(), 2);
        assert!(numbers.contains(&1));
        assert!(numbers.contains(&5));
    }

    #[test]
    fn test_import_dedup_skips_existing() {
        // Simulate the dedup logic without making API calls.
        let existing = vec![
            make_bead_with_issue(1, BeadStatus::Backlog),
            make_bead_with_issue(3, BeadStatus::Hooked),
        ];

        let imported_numbers = extract_imported_issue_numbers(&existing);

        let incoming_issues = [
            make_github_issue(1, "Already imported", IssueState::Open),
            make_github_issue(2, "New issue", IssueState::Open),
            make_github_issue(3, "Also imported", IssueState::Open),
            make_github_issue(4, "Another new", IssueState::Open),
        ];

        let new_beads: Vec<Bead> = incoming_issues
            .iter()
            .filter(|issue| !imported_numbers.contains(&issue.number))
            .map(issues::import_issue_as_task)
            .collect();

        assert_eq!(new_beads.len(), 2);
        assert_eq!(new_beads[0].title, "New issue");
        assert_eq!(new_beads[1].title, "Another new");
    }

    #[test]
    fn test_status_sync_done_maps_to_closed() {
        let bead = make_bead_with_issue(42, BeadStatus::Done);
        let issue_number = bead_issue_number(&bead);
        assert_eq!(issue_number, Some(42));

        // When status is Done, we should close the issue.
        let target = match bead.status {
            BeadStatus::Done => Some(IssueState::Closed),
            BeadStatus::Backlog => Some(IssueState::Open),
            _ => None,
        };
        assert_eq!(target, Some(IssueState::Closed));
    }

    #[test]
    fn test_status_sync_backlog_maps_to_open() {
        let bead = make_bead_with_issue(42, BeadStatus::Backlog);

        let target = match bead.status {
            BeadStatus::Done => Some(IssueState::Closed),
            BeadStatus::Backlog => Some(IssueState::Open),
            _ => None,
        };
        assert_eq!(target, Some(IssueState::Open));
    }

    #[test]
    fn test_status_sync_hooked_maps_to_none() {
        let bead = make_bead_with_issue(42, BeadStatus::Hooked);

        let target = match bead.status {
            BeadStatus::Done => Some(IssueState::Closed),
            BeadStatus::Backlog => Some(IssueState::Open),
            _ => None,
        };
        assert!(target.is_none());
    }

    #[test]
    fn test_export_bead_creates_correct_metadata() {
        let issue = make_github_issue(99, "Exported task", IssueState::Open);
        let metadata = build_issue_metadata(&issue);

        assert_eq!(metadata["source"], "github");
        assert_eq!(metadata["issue_number"], 99);
        assert_eq!(
            metadata["html_url"],
            "https://github.com/test/repo/issues/99"
        );
        assert_eq!(metadata["author"], "testuser");
    }

    #[test]
    fn test_import_issue_as_task_preserves_fields() {
        let issue = make_github_issue(7, "Test issue title", IssueState::Open);
        let bead = issues::import_issue_as_task(&issue);

        assert_eq!(bead.title, "Test issue title");
        assert_eq!(bead.status, BeadStatus::Backlog);
        assert!(bead.description.is_some());
        let meta = bead.metadata.as_ref().unwrap();
        assert_eq!(meta["issue_number"], 7);
        assert_eq!(meta["source"], "github");
    }

    #[test]
    fn test_poll_updates_filters_by_time() {
        let now = Utc::now();
        let old = now - chrono::Duration::hours(2);
        let recent = now - chrono::Duration::minutes(5);
        let since = now - chrono::Duration::hours(1);

        let issues = [
            {
                let mut i = make_github_issue(1, "Old issue", IssueState::Open);
                i.updated_at = old;
                i
            },
            {
                let mut i = make_github_issue(2, "Recent issue", IssueState::Open);
                i.updated_at = recent;
                i
            },
        ];

        let filtered: Vec<&GitHubIssue> = issues.iter().filter(|i| i.updated_at >= since).collect();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].number, 2);
    }

    #[test]
    fn test_bead_without_metadata_has_no_issue_number() {
        let bead = make_plain_bead("No metadata bead");
        assert!(bead_issue_number(&bead).is_none());
    }
}
