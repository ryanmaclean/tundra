pub mod github;
pub mod types;

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::github::client::GitHubClient;
    use crate::github::issues::import_issue_as_task;
    use crate::types::*;

    // ---- Type serialization roundtrips ----

    #[test]
    fn github_issue_serde_roundtrip() {
        let issue = GitHubIssue {
            number: 42,
            title: "Fix the widget".to_string(),
            body: Some("It is broken".to_string()),
            state: IssueState::Open,
            labels: vec![GitHubLabel {
                name: "bug".to_string(),
                color: "d73a4a".to_string(),
                description: Some("Something isn't working".to_string()),
            }],
            assignees: vec!["alice".to_string()],
            author: "bob".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            comments: 3,
            html_url: "https://github.com/owner/repo/issues/42".to_string(),
        };

        let json = serde_json::to_string(&issue).unwrap();
        let deserialized: GitHubIssue = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.number, 42);
        assert_eq!(deserialized.title, "Fix the widget");
        assert_eq!(deserialized.state, IssueState::Open);
        assert_eq!(deserialized.labels.len(), 1);
        assert_eq!(deserialized.labels[0].name, "bug");
    }

    #[test]
    fn github_pr_serde_roundtrip() {
        let pr = GitHubPullRequest {
            number: 101,
            title: "Add feature X".to_string(),
            body: Some("Implements feature X".to_string()),
            state: PrState::Open,
            author: "alice".to_string(),
            head_branch: "feature-x".to_string(),
            base_branch: "main".to_string(),
            labels: vec![],
            reviewers: vec!["bob".to_string()],
            draft: false,
            mergeable: Some(true),
            additions: 50,
            deletions: 10,
            changed_files: 3,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            merged_at: None,
            html_url: "https://github.com/owner/repo/pull/101".to_string(),
        };

        let json = serde_json::to_string(&pr).unwrap();
        let deserialized: GitHubPullRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.number, 101);
        assert_eq!(deserialized.state, PrState::Open);
        assert_eq!(deserialized.head_branch, "feature-x");
        assert_eq!(deserialized.additions, 50);
    }

    #[test]
    fn issue_state_serde() {
        let open_json = serde_json::to_string(&IssueState::Open).unwrap();
        assert_eq!(open_json, "\"open\"");
        let closed_json = serde_json::to_string(&IssueState::Closed).unwrap();
        assert_eq!(closed_json, "\"closed\"");

        let open: IssueState = serde_json::from_str("\"open\"").unwrap();
        assert_eq!(open, IssueState::Open);
    }

    #[test]
    fn pr_state_serde() {
        let merged_json = serde_json::to_string(&PrState::Merged).unwrap();
        assert_eq!(merged_json, "\"merged\"");

        let merged: PrState = serde_json::from_str("\"merged\"").unwrap();
        assert_eq!(merged, PrState::Merged);
    }

    #[test]
    fn finding_severity_serde() {
        let crit = serde_json::to_string(&FindingSeverity::Critical).unwrap();
        assert_eq!(crit, "\"critical\"");

        let info: FindingSeverity = serde_json::from_str("\"info\"").unwrap();
        assert_eq!(info, FindingSeverity::Info);
    }

    #[test]
    fn github_config_serde_roundtrip() {
        let config = GitHubConfig {
            token: Some("ghp_test123".to_string()),
            owner: "myorg".to_string(),
            repo: "myrepo".to_string(),
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: GitHubConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.owner, "myorg");
        assert_eq!(deserialized.repo, "myrepo");
        assert_eq!(deserialized.token.unwrap(), "ghp_test123");
    }

    #[test]
    fn github_release_serde_roundtrip() {
        let release = GitHubRelease {
            tag_name: "v1.0.0".to_string(),
            name: Some("Version 1.0".to_string()),
            body: Some("Release notes".to_string()),
            draft: false,
            prerelease: false,
            created_at: Utc::now(),
            html_url: "https://github.com/owner/repo/releases/tag/v1.0.0".to_string(),
        };

        let json = serde_json::to_string(&release).unwrap();
        let deserialized: GitHubRelease = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.tag_name, "v1.0.0");
        assert!(!deserialized.draft);
    }

    #[test]
    fn review_finding_serde_roundtrip() {
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
    }

    // ---- GitHubClient creation ----

    #[tokio::test]
    async fn client_creation_with_config() {
        let config = GitHubConfig {
            token: Some("ghp_test_token".to_string()),
            owner: "testowner".to_string(),
            repo: "testrepo".to_string(),
        };

        let client = GitHubClient::new(config).unwrap();
        assert_eq!(client.owner(), "testowner");
        assert_eq!(client.repo(), "testrepo");
    }

    #[test]
    fn client_creation_missing_token() {
        let config = GitHubConfig {
            token: None,
            owner: "testowner".to_string(),
            repo: "testrepo".to_string(),
        };

        let result = GitHubClient::new(config);
        assert!(result.is_err());
    }

    // ---- Issue-to-bead conversion ----

    #[test]
    fn import_open_issue_as_bead() {
        let now = Utc::now();
        let issue = GitHubIssue {
            number: 7,
            title: "Add logging".to_string(),
            body: Some("We need better logging".to_string()),
            state: IssueState::Open,
            labels: vec![GitHubLabel {
                name: "enhancement".to_string(),
                color: "a2eeef".to_string(),
                description: None,
            }],
            assignees: vec![],
            author: "dev".to_string(),
            created_at: now,
            updated_at: now,
            comments: 0,
            html_url: "https://github.com/owner/repo/issues/7".to_string(),
        };

        let bead = import_issue_as_task(&issue);

        assert_eq!(bead.title, "Add logging");
        assert_eq!(bead.description.as_deref(), Some("We need better logging"));
        assert_eq!(bead.status, at_core::types::BeadStatus::Backlog);
        assert_eq!(bead.lane, at_core::types::Lane::Standard);
        assert!(bead.done_at.is_none());

        // Verify metadata contains source info
        let meta = bead.metadata.unwrap();
        assert_eq!(meta["source"], "github");
        assert_eq!(meta["issue_number"], 7);
    }

    #[test]
    fn import_closed_issue_as_bead() {
        let now = Utc::now();
        let issue = GitHubIssue {
            number: 12,
            title: "Fix crash".to_string(),
            body: None,
            state: IssueState::Closed,
            labels: vec![],
            assignees: vec![],
            author: "dev".to_string(),
            created_at: now,
            updated_at: now,
            comments: 5,
            html_url: "https://github.com/owner/repo/issues/12".to_string(),
        };

        let bead = import_issue_as_task(&issue);

        assert_eq!(bead.status, at_core::types::BeadStatus::Done);
        assert!(bead.done_at.is_some());
    }
}
