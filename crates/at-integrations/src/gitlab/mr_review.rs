//! GitLab Merge Request review engine.
//!
//! Provides automated code review capabilities for GitLab merge requests,
//! producing structured findings with severity levels and suggestions.
//!
//! When a [`GitLabClient`] is provided, the engine fetches real MR diffs from
//! the GitLab API and runs heuristic analysis on changed files. Without a
//! client (or in tests), it falls back to stub findings for pipeline validation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::GitLabClient;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the MR review engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrReviewConfig {
    /// Minimum severity to include in results.
    pub severity_threshold: MrReviewSeverity,
    /// Maximum number of findings to report.
    pub max_findings: usize,
    /// Whether to auto-approve MRs that pass the review.
    pub auto_approve: bool,
}

impl Default for MrReviewConfig {
    fn default() -> Self {
        Self {
            severity_threshold: MrReviewSeverity::Low,
            max_findings: 50,
            auto_approve: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Severity level for a review finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MrReviewSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for MrReviewSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Info => "info",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        };
        write!(f, "{label}")
    }
}

/// A single finding from the review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrReviewFinding {
    /// File path relative to the repository root.
    pub file: String,
    /// Line number where the finding was detected (1-based).
    pub line: u32,
    /// Severity of the finding.
    pub severity: MrReviewSeverity,
    /// Category such as "style", "security", "performance", "correctness".
    pub category: String,
    /// Human-readable description of the finding.
    pub message: String,
    /// Optional suggested replacement or fix.
    pub suggestion: Option<String>,
}

/// The result of reviewing a merge request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrReviewResult {
    /// Individual findings from the review.
    pub findings: Vec<MrReviewFinding>,
    /// Overall summary of the review.
    pub summary: String,
    /// Whether the MR is approved.
    pub approved: bool,
    /// Timestamp when the review was completed.
    pub reviewed_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// GitLab MR changes response
// ---------------------------------------------------------------------------

/// A single changed file from the GitLab MR changes API.
#[derive(Debug, Deserialize)]
struct MrChange {
    new_path: String,
    diff: String,
    new_file: bool,
    deleted_file: bool,
}

/// Response from `GET /projects/:id/merge_requests/:iid/changes`.
#[derive(Debug, Deserialize)]
struct MrChangesResponse {
    changes: Vec<MrChange>,
}

// ---------------------------------------------------------------------------
// Heuristic analysis patterns
// ---------------------------------------------------------------------------

/// A pattern rule for heuristic diff analysis.
struct HeuristicRule {
    pattern: &'static str,
    severity: MrReviewSeverity,
    category: &'static str,
    message: &'static str,
    suggestion: Option<&'static str>,
}

/// Built-in heuristic rules for common code quality issues.
fn heuristic_rules() -> Vec<HeuristicRule> {
    vec![
        HeuristicRule {
            pattern: ".unwrap()",
            severity: MrReviewSeverity::High,
            category: "correctness",
            message: "Unwrap on Result/Option that can fail in production",
            suggestion: Some("Use proper error handling with `?` operator or `unwrap_or`"),
        },
        HeuristicRule {
            pattern: "todo!()",
            severity: MrReviewSeverity::Medium,
            category: "completeness",
            message: "TODO macro will panic at runtime",
            suggestion: Some("Replace with actual implementation or return an error"),
        },
        HeuristicRule {
            pattern: "unimplemented!()",
            severity: MrReviewSeverity::High,
            category: "completeness",
            message: "Unimplemented macro will panic at runtime",
            suggestion: Some("Implement the functionality or return a proper error"),
        },
        HeuristicRule {
            pattern: "panic!(",
            severity: MrReviewSeverity::High,
            category: "correctness",
            message: "Explicit panic in production code",
            suggestion: Some("Return an error instead of panicking"),
        },
        HeuristicRule {
            pattern: "unsafe {",
            severity: MrReviewSeverity::High,
            category: "security",
            message: "Unsafe block introduced — verify memory safety",
            suggestion: Some("Document safety invariants with a // SAFETY: comment"),
        },
        HeuristicRule {
            pattern: "FIXME",
            severity: MrReviewSeverity::Low,
            category: "style",
            message: "FIXME comment indicates known issue",
            suggestion: Some("Consider fixing before merge or creating a tracking issue"),
        },
        HeuristicRule {
            pattern: "dbg!(",
            severity: MrReviewSeverity::Medium,
            category: "style",
            message: "Debug macro left in code",
            suggestion: Some("Remove dbg!() calls before merging"),
        },
        HeuristicRule {
            pattern: "println!(",
            severity: MrReviewSeverity::Low,
            category: "style",
            message: "println! in library/production code — use tracing or log instead",
            suggestion: Some("Replace with tracing::info!() or tracing::debug!()"),
        },
        HeuristicRule {
            pattern: "password",
            severity: MrReviewSeverity::Medium,
            category: "security",
            message: "Reference to 'password' — verify no secrets are hardcoded",
            suggestion: Some("Use environment variables or a secrets manager for credentials"),
        },
        HeuristicRule {
            pattern: "secret",
            severity: MrReviewSeverity::Medium,
            category: "security",
            message: "Reference to 'secret' — verify no secrets are hardcoded",
            suggestion: Some("Use environment variables or a secrets manager for secrets"),
        },
    ]
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// Engine that performs automated reviews of GitLab merge requests.
///
/// When constructed with a [`GitLabClient`], the engine fetches real MR diffs
/// from the GitLab API and runs heuristic analysis. Without a client, it
/// returns stub findings for testing.
pub struct MrReviewEngine {
    config: MrReviewConfig,
    client: Option<GitLabClient>,
}

impl MrReviewEngine {
    /// Create a new review engine with the given configuration and client.
    pub fn new(config: MrReviewConfig) -> Self {
        Self {
            config,
            client: None,
        }
    }

    /// Create a review engine with a GitLab client for real API access.
    pub fn with_client(config: MrReviewConfig, client: GitLabClient) -> Self {
        Self {
            config,
            client: Some(client),
        }
    }

    /// Create a review engine with default configuration (no client).
    pub fn with_defaults() -> Self {
        Self::new(MrReviewConfig::default())
    }

    /// Return a reference to the current configuration.
    pub fn config(&self) -> &MrReviewConfig {
        &self.config
    }

    /// Review a merge request.
    ///
    /// With a real GitLab client: fetches MR changes from the API and runs
    /// heuristic pattern analysis on the diffs. Without a client (or if the
    /// client uses a test token): returns stub findings for pipeline validation.
    pub async fn review_mr(&self, project_id: &str, mr_iid: u32) -> MrReviewResult {
        let all_findings = match &self.client {
            Some(client) if !client.is_stub_token() => {
                self.review_real(client, project_id, mr_iid).await
            }
            _ => Self::stub_findings(),
        };

        self.build_result(all_findings)
    }

    /// Fetch real MR changes and analyze them with heuristic rules.
    async fn review_real(
        &self,
        client: &GitLabClient,
        project_id: &str,
        mr_iid: u32,
    ) -> Vec<MrReviewFinding> {
        let path = format!("/projects/{}/merge_requests/{}/changes", project_id, mr_iid);

        let response = match client.api_get(&path).await {
            Ok(resp) => resp,
            Err(e) => {
                // If we can't fetch the MR, return a single error finding.
                return vec![MrReviewFinding {
                    file: "(api)".into(),
                    line: 0,
                    severity: MrReviewSeverity::Info,
                    category: "review".into(),
                    message: format!("Could not fetch MR changes: {e}"),
                    suggestion: Some("Check GitLab token permissions and project ID".into()),
                }];
            }
        };

        let changes: MrChangesResponse = match response.json().await {
            Ok(c) => c,
            Err(e) => {
                return vec![MrReviewFinding {
                    file: "(api)".into(),
                    line: 0,
                    severity: MrReviewSeverity::Info,
                    category: "review".into(),
                    message: format!("Failed to parse MR changes response: {e}"),
                    suggestion: None,
                }];
            }
        };

        let rules = heuristic_rules();
        let mut findings = Vec::new();

        for change in &changes.changes {
            // Skip deleted files — nothing to review.
            if change.deleted_file {
                continue;
            }

            // Analyze added/modified lines in the diff.
            for (line_in_diff, line_text) in change.diff.lines().enumerate() {
                // Only check lines that are additions (start with '+').
                if !line_text.starts_with('+') || line_text.starts_with("+++") {
                    continue;
                }

                let content = &line_text[1..]; // strip leading '+'

                for rule in &rules {
                    if content.contains(rule.pattern) {
                        findings.push(MrReviewFinding {
                            file: change.new_path.clone(),
                            line: (line_in_diff + 1) as u32,
                            severity: rule.severity,
                            category: rule.category.into(),
                            message: rule.message.into(),
                            suggestion: rule.suggestion.map(Into::into),
                        });
                    }
                }
            }

            // Flag very large new files.
            if change.new_file {
                let line_count = change.diff.lines().count();
                if line_count > 500 {
                    findings.push(MrReviewFinding {
                        file: change.new_path.clone(),
                        line: 1,
                        severity: MrReviewSeverity::Low,
                        category: "style".into(),
                        message: format!("New file is very large ({line_count} lines in diff)"),
                        suggestion: Some("Consider splitting into smaller modules".into()),
                    });
                }
            }
        }

        findings
    }

    /// Stub findings for testing without a real GitLab API.
    fn stub_findings() -> Vec<MrReviewFinding> {
        vec![
            MrReviewFinding {
                file: "src/main.rs".into(),
                line: 42,
                severity: MrReviewSeverity::High,
                category: "security".into(),
                message: "Potential SQL injection via unsanitized user input".into(),
                suggestion: Some(
                    "Use parameterized queries instead of string concatenation".into(),
                ),
            },
            MrReviewFinding {
                file: "src/lib.rs".into(),
                line: 15,
                severity: MrReviewSeverity::Medium,
                category: "performance".into(),
                message: "Unnecessary clone of large struct in hot path".into(),
                suggestion: Some("Pass by reference instead of cloning".into()),
            },
            MrReviewFinding {
                file: "src/utils.rs".into(),
                line: 88,
                severity: MrReviewSeverity::Low,
                category: "style".into(),
                message: "Function exceeds recommended line count (120 lines)".into(),
                suggestion: Some("Consider splitting into smaller functions".into()),
            },
            MrReviewFinding {
                file: "src/config.rs".into(),
                line: 3,
                severity: MrReviewSeverity::Info,
                category: "style".into(),
                message: "Unused import detected".into(),
                suggestion: Some("Remove the unused import".into()),
            },
            MrReviewFinding {
                file: "src/handler.rs".into(),
                line: 67,
                severity: MrReviewSeverity::Critical,
                category: "correctness".into(),
                message: "Unwrap on Result that can fail in production".into(),
                suggestion: Some("Use proper error handling with `?` operator".into()),
            },
        ]
    }

    /// Build the final review result from raw findings, applying config filters.
    fn build_result(&self, all_findings: Vec<MrReviewFinding>) -> MrReviewResult {
        let findings: Vec<MrReviewFinding> = all_findings
            .into_iter()
            .filter(|f| f.severity >= self.config.severity_threshold)
            .take(self.config.max_findings)
            .collect();

        let has_critical = findings
            .iter()
            .any(|f| f.severity >= MrReviewSeverity::High);

        let approved = self.config.auto_approve && !has_critical;

        let summary = if findings.is_empty() {
            "No findings. The merge request looks good.".to_string()
        } else {
            format!(
                "Found {} issue(s) across reviewed files. {} critical/high severity.",
                findings.len(),
                findings
                    .iter()
                    .filter(|f| f.severity >= MrReviewSeverity::High)
                    .count()
            )
        };

        MrReviewResult {
            findings,
            summary,
            approved,
            reviewed_at: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = MrReviewConfig::default();
        assert_eq!(config.severity_threshold, MrReviewSeverity::Low);
        assert_eq!(config.max_findings, 50);
        assert!(!config.auto_approve);
    }

    #[test]
    fn severity_ordering() {
        assert!(MrReviewSeverity::Critical > MrReviewSeverity::High);
        assert!(MrReviewSeverity::High > MrReviewSeverity::Medium);
        assert!(MrReviewSeverity::Medium > MrReviewSeverity::Low);
        assert!(MrReviewSeverity::Low > MrReviewSeverity::Info);
    }

    #[test]
    fn severity_display() {
        assert_eq!(MrReviewSeverity::Critical.to_string(), "critical");
        assert_eq!(MrReviewSeverity::Info.to_string(), "info");
    }

    #[test]
    fn config_serde_roundtrip() {
        let config = MrReviewConfig {
            severity_threshold: MrReviewSeverity::Medium,
            max_findings: 25,
            auto_approve: true,
        };

        let json = serde_json::to_string(&config).unwrap();
        let de: MrReviewConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(de.severity_threshold, MrReviewSeverity::Medium);
        assert_eq!(de.max_findings, 25);
        assert!(de.auto_approve);
    }

    #[test]
    fn finding_serde_roundtrip() {
        let finding = MrReviewFinding {
            file: "src/main.rs".into(),
            line: 42,
            severity: MrReviewSeverity::High,
            category: "security".into(),
            message: "SQL injection risk".into(),
            suggestion: Some("Use parameterized queries".into()),
        };

        let json = serde_json::to_string(&finding).unwrap();
        let de: MrReviewFinding = serde_json::from_str(&json).unwrap();
        assert_eq!(de.file, "src/main.rs");
        assert_eq!(de.line, 42);
        assert_eq!(de.severity, MrReviewSeverity::High);
        assert_eq!(de.category, "security");
    }

    #[test]
    fn finding_without_suggestion() {
        let json = r#"{
            "file": "src/lib.rs",
            "line": 10,
            "severity": "low",
            "category": "style",
            "message": "Long line",
            "suggestion": null
        }"#;

        let finding: MrReviewFinding = serde_json::from_str(json).unwrap();
        assert!(finding.suggestion.is_none());
    }

    #[test]
    fn review_result_serde_roundtrip() {
        let result = MrReviewResult {
            findings: vec![MrReviewFinding {
                file: "test.rs".into(),
                line: 1,
                severity: MrReviewSeverity::Info,
                category: "style".into(),
                message: "Minor issue".into(),
                suggestion: None,
            }],
            summary: "1 finding".into(),
            approved: true,
            reviewed_at: Utc::now(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let de: MrReviewResult = serde_json::from_str(&json).unwrap();
        assert_eq!(de.findings.len(), 1);
        assert!(de.approved);
    }

    #[tokio::test]
    async fn stub_review_returns_findings() {
        let engine = MrReviewEngine::with_defaults();
        let result = engine.review_mr("42", 1).await;

        assert!(!result.findings.is_empty());
        assert!(!result.summary.is_empty());
        assert!(!result.approved); // auto_approve is false by default
    }

    #[tokio::test]
    async fn stub_review_respects_severity_threshold() {
        let config = MrReviewConfig {
            severity_threshold: MrReviewSeverity::High,
            max_findings: 50,
            auto_approve: false,
        };
        let engine = MrReviewEngine::new(config);
        let result = engine.review_mr("42", 1).await;

        for finding in &result.findings {
            assert!(finding.severity >= MrReviewSeverity::High);
        }
    }

    #[tokio::test]
    async fn stub_review_respects_max_findings() {
        let config = MrReviewConfig {
            severity_threshold: MrReviewSeverity::Info,
            max_findings: 2,
            auto_approve: false,
        };
        let engine = MrReviewEngine::new(config);
        let result = engine.review_mr("42", 1).await;

        assert!(result.findings.len() <= 2);
    }

    #[tokio::test]
    async fn stub_review_auto_approve_blocked_by_critical() {
        let config = MrReviewConfig {
            severity_threshold: MrReviewSeverity::Info,
            max_findings: 50,
            auto_approve: true,
        };
        let engine = MrReviewEngine::new(config);
        let result = engine.review_mr("42", 1).await;

        // The stub data includes critical/high findings, so auto-approve should be blocked.
        assert!(!result.approved);
    }

    #[tokio::test]
    async fn stub_review_auto_approve_when_no_critical() {
        let config = MrReviewConfig {
            severity_threshold: MrReviewSeverity::Info,
            max_findings: 50,
            auto_approve: true,
        };
        let engine = MrReviewEngine::new(config);

        // Filter to only info-level to simulate no critical issues:
        // We use a high threshold so no findings pass, then auto_approve should
        // succeed since there are no critical findings in an empty set.
        let config2 = MrReviewConfig {
            severity_threshold: MrReviewSeverity::Critical,
            max_findings: 0,
            auto_approve: true,
        };
        let engine2 = MrReviewEngine::new(config2);
        let result = engine2.review_mr("42", 1).await;
        assert!(result.approved);

        // Suppress unused warning
        let _ = engine.config();
    }

    #[test]
    fn severity_serde_values() {
        let json = serde_json::to_string(&MrReviewSeverity::Critical).unwrap();
        assert_eq!(json, r#""critical""#);

        let de: MrReviewSeverity = serde_json::from_str(r#""medium""#).unwrap();
        assert_eq!(de, MrReviewSeverity::Medium);
    }

    #[test]
    fn heuristic_rules_are_valid() {
        let rules = heuristic_rules();
        assert!(!rules.is_empty());
        for rule in &rules {
            assert!(!rule.pattern.is_empty());
            assert!(!rule.message.is_empty());
            assert!(!rule.category.is_empty());
        }
    }

    #[test]
    fn build_result_filters_and_caps() {
        let engine = MrReviewEngine::new(MrReviewConfig {
            severity_threshold: MrReviewSeverity::Medium,
            max_findings: 2,
            auto_approve: false,
        });

        let findings = vec![
            MrReviewFinding {
                file: "a.rs".into(),
                line: 1,
                severity: MrReviewSeverity::Low,
                category: "style".into(),
                message: "low".into(),
                suggestion: None,
            },
            MrReviewFinding {
                file: "b.rs".into(),
                line: 2,
                severity: MrReviewSeverity::Medium,
                category: "style".into(),
                message: "med".into(),
                suggestion: None,
            },
            MrReviewFinding {
                file: "c.rs".into(),
                line: 3,
                severity: MrReviewSeverity::High,
                category: "correctness".into(),
                message: "high".into(),
                suggestion: None,
            },
            MrReviewFinding {
                file: "d.rs".into(),
                line: 4,
                severity: MrReviewSeverity::Critical,
                category: "security".into(),
                message: "crit".into(),
                suggestion: None,
            },
        ];

        let result = engine.build_result(findings);
        // Low is filtered out (below Medium threshold), and max 2 findings.
        assert_eq!(result.findings.len(), 2);
        assert!(result
            .findings
            .iter()
            .all(|f| f.severity >= MrReviewSeverity::Medium));
    }

    #[test]
    fn engine_with_client_creates_properly() {
        let client =
            GitLabClient::new_with_url("https://gitlab.example.com", "stub-token").unwrap();
        let engine = MrReviewEngine::with_client(MrReviewConfig::default(), client);
        assert!(engine.client.is_some());
    }
}
