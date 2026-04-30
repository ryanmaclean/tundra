//! Shared API response types for auto-tundra services.
//!
//! This crate provides common type definitions used across multiple services
//! to ensure consistency in API responses and reduce code duplication.

use serde::{Deserialize, Serialize};

// ── Core API response types (matching backend JSON) ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiBead {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub lane: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub priority_label: Option<String>,
    #[serde(default)]
    pub agent_profile: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub thinking_level: Option<String>,
    #[serde(default)]
    pub complexity: Option<String>,
    #[serde(default)]
    pub impact: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiAgent {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ApiKpi {
    #[serde(default)]
    pub total_beads: u64,
    #[serde(default)]
    pub backlog: u64,
    #[serde(default)]
    pub hooked: u64,
    #[serde(default)]
    pub slung: u64,
    #[serde(default)]
    pub review: u64,
    #[serde(default)]
    pub done: u64,
    #[serde(default)]
    pub failed: u64,
    #[serde(default)]
    pub active_agents: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiSession {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub agent_name: String,
    #[serde(default)]
    pub cli_type: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub duration: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiConvoy {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub bead_count: u32,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub bead_ids: Vec<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiWorktree {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub bead_id: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ApiCosts {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub sessions: Vec<ApiCostSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiCostSession {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub agent_name: String,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiMcpServer {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiMemoryEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub created_at: String,
}

/// A single feature within a roadmap (matches backend `RoadmapFeature`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiRoadmapFeature {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub estimated_effort: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub created_at: String,
}

/// A roadmap container with nested features (matches backend `Roadmap`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiRoadmap {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub features: Vec<ApiRoadmapFeature>,
    #[serde(default)]
    pub generated_at: String,
}

/// Flat roadmap item used by the UI after flattening nested roadmaps.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiRoadmapItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiIdea {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub impact: String,
    #[serde(default)]
    pub effort: String,
}

// ── Stack types (stacked diffs) ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiStackNode {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub pr_number: Option<u32>,
    #[serde(default)]
    pub stack_position: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiStack {
    pub root: ApiStackNode,
    #[serde(default)]
    pub children: Vec<ApiStackNode>,
    #[serde(default)]
    pub total: u32,
}

// ── GitHub API types ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiGithubIssue {
    #[serde(default)]
    pub number: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiGithubPr {
    #[serde(default)]
    pub number: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub reviewers: Vec<String>,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub created_at: Option<String>,
}

// ── Changelog API types ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiChangelogSection {
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiChangelogEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub sections: Vec<ApiChangelogSection>,
}

// ── API request types ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateBeadRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lane: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateStatusRequest {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AddMemoryRequest {
    pub key: String,
    pub value: String,
    pub category: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SendInsightsMessageRequest {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AddMcpServerRequest {
    pub name: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReviewGitLabMrRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity_threshold: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateTaskRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub bead_id: String,
    pub priority: String,
    pub complexity: String,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenerateChangelogRequest {
    pub commits: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PublishGithubReleaseRequest {
    pub tag_name: String,
    pub name: String,
    pub body: String,
    pub draft: bool,
    pub prerelease: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AddRoadmapFeatureRequest {
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SendInsightsMessageWithModelRequest {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateProjectRequest {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateProjectRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[cfg(test)]
mod serde_tests {
    use super::*;
    use proptest::prelude::*;
    use serde::de::DeserializeOwned;
    use sha2::{Digest, Sha256};
    use std::fmt::Debug;

    fn roundtrip<T: Serialize + DeserializeOwned + PartialEq + Debug>(v: &T) {
        let json = serde_json::to_string(v).expect("serialize");
        let back: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(v, &back, "JSON round-trip mismatch");
    }

    // ── Section A: explicit per-type round-trip ──────────────────────────────

    #[test]
    fn rt_api_bead() {
        roundtrip(&ApiBead {
            id: "b-1".into(),
            title: "title".into(),
            description: Some("desc".into()),
            status: "queued".into(),
            lane: "default".into(),
            priority: 7,
            category: Some("infra".into()),
            priority_label: Some("p1".into()),
            agent_profile: Some("default".into()),
            model: Some("claude-opus".into()),
            thinking_level: Some("high".into()),
            complexity: Some("M".into()),
            impact: Some("high".into()),
            effort: Some("3d".into()),
            metadata: Some(serde_json::json!({"k": "v", "n": 42})),
        });
    }

    #[test]
    fn rt_api_agent() {
        roundtrip(&ApiAgent {
            id: "a-1".into(),
            name: "agent".into(),
            role: "worker".into(),
            status: "idle".into(),
        });
    }

    #[test]
    fn rt_api_kpi() {
        roundtrip(&ApiKpi {
            total_beads: 100,
            backlog: 10,
            hooked: 5,
            slung: 20,
            review: 15,
            done: 45,
            failed: 5,
            active_agents: 3,
        });
    }

    #[test]
    fn rt_api_kpi_default() {
        roundtrip(&ApiKpi::default());
    }

    #[test]
    fn rt_api_session() {
        roundtrip(&ApiSession {
            id: "s-1".into(),
            agent_name: "claude".into(),
            cli_type: "claude-code".into(),
            status: "running".into(),
            duration: "10m".into(),
        });
    }

    #[test]
    fn rt_api_convoy() {
        roundtrip(&ApiConvoy {
            id: "c-1".into(),
            name: "convoy".into(),
            bead_count: 4,
            status: "active".into(),
            bead_ids: vec!["b-1".into(), "b-2".into()],
            created_at: Some("2026-01-01".into()),
            updated_at: Some("2026-01-02".into()),
            metadata: Some(serde_json::json!({"k": "v"})),
        });
    }

    // Documents a serde quirk: `Some(Value::Null)` round-trips through
    // `Option<Value>` with `#[serde(default)]` and becomes `None`. Pin this
    // behavior so a future serde upgrade can't silently change it.
    #[test]
    fn convoy_metadata_some_null_collapses_to_none() {
        let v = ApiConvoy {
            id: "c".into(),
            name: "n".into(),
            bead_count: 0,
            status: "".into(),
            bead_ids: vec![],
            created_at: None,
            updated_at: None,
            metadata: Some(serde_json::Value::Null),
        };
        let back: ApiConvoy = serde_json::from_str(&serde_json::to_string(&v).unwrap()).unwrap();
        assert!(back.metadata.is_none());
    }

    #[test]
    fn rt_api_worktree() {
        roundtrip(&ApiWorktree {
            id: "w-1".into(),
            path: "/tmp/wt".into(),
            branch: "feature".into(),
            bead_id: "b-1".into(),
            status: "open".into(),
        });
    }

    #[test]
    fn rt_api_costs() {
        roundtrip(&ApiCosts {
            input_tokens: 1_000,
            output_tokens: 500,
            sessions: vec![ApiCostSession {
                session_id: "s-1".into(),
                agent_name: "claude".into(),
                input_tokens: 100,
                output_tokens: 50,
            }],
        });
    }

    #[test]
    fn rt_api_costs_default() {
        roundtrip(&ApiCosts::default());
    }

    #[test]
    fn rt_api_cost_session() {
        roundtrip(&ApiCostSession {
            session_id: "s-1".into(),
            agent_name: "claude".into(),
            input_tokens: 1,
            output_tokens: 2,
        });
    }

    #[test]
    fn rt_api_mcp_server() {
        roundtrip(&ApiMcpServer {
            name: "fs".into(),
            status: "ready".into(),
            tools: vec!["read".into(), "write".into()],
        });
    }

    #[test]
    fn rt_api_memory_entry() {
        roundtrip(&ApiMemoryEntry {
            id: "m-1".into(),
            category: "preference".into(),
            content: "use snake_case".into(),
            created_at: "2026-04-27T00:00:00Z".into(),
        });
    }

    #[test]
    fn rt_api_roadmap_feature() {
        roundtrip(&ApiRoadmapFeature {
            id: "rf-1".into(),
            title: "feature".into(),
            description: "do stuff".into(),
            status: "planned".into(),
            priority: 1,
            estimated_effort: "1d".into(),
            dependencies: vec!["rf-0".into()],
            created_at: "2026-04-27T00:00:00Z".into(),
        });
    }

    #[test]
    fn rt_api_roadmap() {
        roundtrip(&ApiRoadmap {
            id: "r-1".into(),
            name: "q2".into(),
            features: vec![ApiRoadmapFeature {
                id: "rf-1".into(),
                title: "feature".into(),
                description: "".into(),
                status: "planned".into(),
                priority: 0,
                estimated_effort: "".into(),
                dependencies: vec![],
                created_at: "".into(),
            }],
            generated_at: "2026-04-27T00:00:00Z".into(),
        });
    }

    #[test]
    fn rt_api_roadmap_item() {
        roundtrip(&ApiRoadmapItem {
            id: "ri-1".into(),
            title: "t".into(),
            description: "d".into(),
            status: "open".into(),
            priority: "p1".into(),
        });
    }

    #[test]
    fn rt_api_idea() {
        roundtrip(&ApiIdea {
            id: "i-1".into(),
            title: "idea".into(),
            description: "desc".into(),
            category: "infra".into(),
            impact: "high".into(),
            effort: "low".into(),
        });
    }

    #[test]
    fn rt_api_stack_node() {
        roundtrip(&ApiStackNode {
            id: "sn-1".into(),
            title: "root".into(),
            phase: "draft".into(),
            git_branch: Some("feature/x".into()),
            pr_number: Some(42),
            stack_position: 0,
        });
    }

    #[test]
    fn rt_api_stack() {
        roundtrip(&ApiStack {
            root: ApiStackNode {
                id: "sn-1".into(),
                title: "root".into(),
                phase: "draft".into(),
                git_branch: None,
                pr_number: None,
                stack_position: 0,
            },
            children: vec![ApiStackNode {
                id: "sn-2".into(),
                title: "child".into(),
                phase: "draft".into(),
                git_branch: Some("feature/y".into()),
                pr_number: Some(43),
                stack_position: 1,
            }],
            total: 2,
        });
    }

    #[test]
    fn rt_api_github_issue() {
        roundtrip(&ApiGithubIssue {
            number: 1,
            title: "bug".into(),
            labels: vec!["bug".into()],
            assignee: Some("alice".into()),
            state: "open".into(),
            created: "2026-04-27".into(),
            created_at: Some("2026-04-27T00:00:00Z".into()),
        });
    }

    #[test]
    fn rt_api_github_pr() {
        roundtrip(&ApiGithubPr {
            number: 2,
            title: "feat".into(),
            author: "bob".into(),
            status: "open".into(),
            state: Some("open".into()),
            reviewers: vec!["alice".into()],
            created: "2026-04-27".into(),
            created_at: Some("2026-04-27T00:00:00Z".into()),
        });
    }

    #[test]
    fn rt_api_changelog_section() {
        roundtrip(&ApiChangelogSection {
            category: "Added".into(),
            items: vec!["x".into(), "y".into()],
        });
    }

    #[test]
    fn rt_api_changelog_entry() {
        roundtrip(&ApiChangelogEntry {
            id: "cl-1".into(),
            version: "1.0.0".into(),
            date: "2026-04-27".into(),
            sections: vec![ApiChangelogSection {
                category: "Added".into(),
                items: vec!["initial".into()],
            }],
        });
    }

    #[test]
    fn rt_create_bead_request() {
        roundtrip(&CreateBeadRequest {
            title: "t".into(),
            description: Some("d".into()),
            lane: Some("default".into()),
        });
    }

    #[test]
    fn rt_create_bead_request_minimal() {
        roundtrip(&CreateBeadRequest {
            title: "t".into(),
            description: None,
            lane: None,
        });
    }

    #[test]
    fn rt_update_status_request() {
        roundtrip(&UpdateStatusRequest {
            status: "done".into(),
        });
    }

    #[test]
    fn rt_add_memory_request() {
        roundtrip(&AddMemoryRequest {
            key: "k".into(),
            value: "v".into(),
            category: "c".into(),
            source: "user".into(),
        });
    }

    #[test]
    fn rt_send_insights_message_request() {
        roundtrip(&SendInsightsMessageRequest {
            content: "hello".into(),
        });
    }

    #[test]
    fn rt_add_mcp_server_request() {
        roundtrip(&AddMcpServerRequest {
            name: "fs".into(),
            command: "/bin/x".into(),
            args: Some(vec!["--flag".into()]),
        });
    }

    #[test]
    fn rt_review_gitlab_mr_request() {
        roundtrip(&ReviewGitLabMrRequest {
            project_id: Some("123".into()),
            strict: Some(true),
            severity_threshold: Some("warning".into()),
        });
    }

    #[test]
    fn rt_create_task_request() {
        roundtrip(&CreateTaskRequest {
            title: "t".into(),
            description: Some("d".into()),
            bead_id: "b-1".into(),
            priority: "p1".into(),
            complexity: "M".into(),
            category: "infra".into(),
        });
    }

    #[test]
    fn rt_generate_changelog_request() {
        roundtrip(&GenerateChangelogRequest {
            commits: "abc..def".into(),
            version: "1.0.0".into(),
        });
    }

    #[test]
    fn rt_publish_github_release_request() {
        roundtrip(&PublishGithubReleaseRequest {
            tag_name: "v1".into(),
            name: "r1".into(),
            body: "notes".into(),
            draft: false,
            prerelease: true,
        });
    }

    #[test]
    fn rt_add_roadmap_feature_request() {
        roundtrip(&AddRoadmapFeatureRequest {
            title: "t".into(),
            description: "d".into(),
            status: "planned".into(),
            priority: "p1".into(),
        });
    }

    #[test]
    fn rt_send_insights_message_with_model_request() {
        roundtrip(&SendInsightsMessageWithModelRequest {
            content: "hello".into(),
            model: Some("claude-opus".into()),
        });
    }

    #[test]
    fn rt_create_project_request() {
        roundtrip(&CreateProjectRequest {
            name: "p".into(),
            path: "/tmp/p".into(),
        });
    }

    #[test]
    fn rt_update_project_request() {
        roundtrip(&UpdateProjectRequest {
            name: Some("p2".into()),
            path: Some("/tmp/p2".into()),
        });
    }

    // ── Section B: serde wire-format invariants ──────────────────────────────

    #[test]
    fn skip_serializing_if_none_omits_field() {
        // CreateBeadRequest.description is `skip_serializing_if = "Option::is_none"`.
        let v = CreateBeadRequest {
            title: "t".into(),
            description: None,
            lane: None,
        };
        let json = serde_json::to_string(&v).unwrap();
        assert!(
            !json.contains("description"),
            "None field must be omitted: {json}"
        );
        assert!(!json.contains("lane"), "None field must be omitted: {json}");
    }

    #[test]
    fn missing_fields_use_default() {
        // ApiBead has #[serde(default)] on every field — empty object must deserialize.
        let v: ApiBead = serde_json::from_str("{}").unwrap();
        assert_eq!(v.id, "");
        assert_eq!(v.priority, 0);
        assert!(v.metadata.is_none());
    }

    #[test]
    fn missing_fields_use_default_kpi() {
        let v: ApiKpi = serde_json::from_str("{}").unwrap();
        assert_eq!(v, ApiKpi::default());
    }

    // ── Section C: proptest round-trips on simpler types ─────────────────────

    proptest! {
        #![proptest_config(ProptestConfig { cases: 64, .. ProptestConfig::default() })]

        #[test]
        fn pt_rt_api_agent(
            id in ".{0,32}",
            name in ".{0,32}",
            role in ".{0,32}",
            status in ".{0,32}",
        ) {
            roundtrip(&ApiAgent { id, name, role, status });
        }

        #[test]
        fn pt_rt_api_kpi(
            total_beads in any::<u64>(),
            backlog in any::<u64>(),
            hooked in any::<u64>(),
            slung in any::<u64>(),
            review in any::<u64>(),
            done in any::<u64>(),
            failed in any::<u64>(),
            active_agents in any::<u64>(),
        ) {
            roundtrip(&ApiKpi {
                total_beads, backlog, hooked, slung, review, done, failed, active_agents,
            });
        }

        #[test]
        fn pt_rt_api_cost_session(
            session_id in ".{0,32}",
            agent_name in ".{0,32}",
            input_tokens in any::<u64>(),
            output_tokens in any::<u64>(),
        ) {
            roundtrip(&ApiCostSession { session_id, agent_name, input_tokens, output_tokens });
        }

        #[test]
        fn pt_rt_update_status_request(status in ".{0,64}") {
            roundtrip(&UpdateStatusRequest { status });
        }

        #[test]
        fn pt_rt_create_bead_request(
            title in ".{1,32}",
            description in proptest::option::of(".{0,64}"),
            lane in proptest::option::of(".{0,32}"),
        ) {
            roundtrip(&CreateBeadRequest { title, description, lane });
        }
    }

    // ── Section D: schema fingerprint ────────────────────────────────────────
    //
    // Concatenate canonical JSON of one populated instance per top-level type
    // and SHA-256 the result. Any wire-format drift (renamed field, changed
    // serde rename, added/removed type) flips the hash and fails this test.
    // To intentionally update: run the test, copy the printed hash into
    // `EXPECTED_SCHEMA_FINGERPRINT`, and document the change in the PR.

    const EXPECTED_SCHEMA_FINGERPRINT: &str =
        "ee0031b70269b6a0e8bff40007e9146d19c5604d353ee69116af7c3634f8b9a0";

    #[test]
    fn schema_fingerprint_stable() {
        let mut h = Sha256::new();
        let canonical: Vec<String> = vec![
            serde_json::to_string(&canonical::api_bead()).unwrap(),
            serde_json::to_string(&canonical::api_agent()).unwrap(),
            serde_json::to_string(&ApiKpi::default()).unwrap(),
            serde_json::to_string(&canonical::api_session()).unwrap(),
            serde_json::to_string(&canonical::api_convoy()).unwrap(),
            serde_json::to_string(&canonical::api_worktree()).unwrap(),
            serde_json::to_string(&ApiCosts::default()).unwrap(),
            serde_json::to_string(&canonical::api_cost_session()).unwrap(),
            serde_json::to_string(&canonical::api_mcp_server()).unwrap(),
            serde_json::to_string(&canonical::api_memory_entry()).unwrap(),
            serde_json::to_string(&canonical::api_roadmap_feature()).unwrap(),
            serde_json::to_string(&canonical::api_roadmap()).unwrap(),
            serde_json::to_string(&canonical::api_roadmap_item()).unwrap(),
            serde_json::to_string(&canonical::api_idea()).unwrap(),
            serde_json::to_string(&canonical::api_stack_node()).unwrap(),
            serde_json::to_string(&canonical::api_stack()).unwrap(),
            serde_json::to_string(&canonical::api_github_issue()).unwrap(),
            serde_json::to_string(&canonical::api_github_pr()).unwrap(),
            serde_json::to_string(&canonical::api_changelog_section()).unwrap(),
            serde_json::to_string(&canonical::api_changelog_entry()).unwrap(),
            serde_json::to_string(&canonical::create_bead_request()).unwrap(),
            serde_json::to_string(&canonical::update_status_request()).unwrap(),
            serde_json::to_string(&canonical::add_memory_request()).unwrap(),
            serde_json::to_string(&canonical::send_insights_message_request()).unwrap(),
            serde_json::to_string(&canonical::add_mcp_server_request()).unwrap(),
            serde_json::to_string(&canonical::review_gitlab_mr_request()).unwrap(),
            serde_json::to_string(&canonical::create_task_request()).unwrap(),
            serde_json::to_string(&canonical::generate_changelog_request()).unwrap(),
            serde_json::to_string(&canonical::publish_github_release_request()).unwrap(),
            serde_json::to_string(&canonical::add_roadmap_feature_request()).unwrap(),
            serde_json::to_string(&canonical::send_insights_message_with_model_request()).unwrap(),
            serde_json::to_string(&canonical::create_project_request()).unwrap(),
            serde_json::to_string(&canonical::update_project_request()).unwrap(),
        ];
        for line in &canonical {
            h.update(line.as_bytes());
            h.update(b"\n");
        }
        let actual = hex::encode(h.finalize());
        assert_eq!(
            actual, EXPECTED_SCHEMA_FINGERPRINT,
            "wire-format schema fingerprint changed; if intentional, update EXPECTED_SCHEMA_FINGERPRINT to: {actual}"
        );
    }

    mod canonical {
        use super::super::*;

        pub fn api_bead() -> ApiBead {
            ApiBead {
                id: "b".into(),
                title: "t".into(),
                description: Some("d".into()),
                status: "queued".into(),
                lane: "default".into(),
                priority: 1,
                category: Some("c".into()),
                priority_label: Some("p1".into()),
                agent_profile: Some("ap".into()),
                model: Some("m".into()),
                thinking_level: Some("high".into()),
                complexity: Some("M".into()),
                impact: Some("high".into()),
                effort: Some("low".into()),
                metadata: None,
            }
        }
        pub fn api_agent() -> ApiAgent {
            ApiAgent {
                id: "a".into(),
                name: "n".into(),
                role: "r".into(),
                status: "s".into(),
            }
        }
        pub fn api_session() -> ApiSession {
            ApiSession {
                id: "s".into(),
                agent_name: "a".into(),
                cli_type: "c".into(),
                status: "running".into(),
                duration: "1m".into(),
            }
        }
        pub fn api_convoy() -> ApiConvoy {
            ApiConvoy {
                id: "c".into(),
                name: "n".into(),
                bead_count: 1,
                status: "active".into(),
                bead_ids: vec!["b".into()],
                created_at: None,
                updated_at: None,
                metadata: None,
            }
        }
        pub fn api_worktree() -> ApiWorktree {
            ApiWorktree {
                id: "w".into(),
                path: "/tmp".into(),
                branch: "b".into(),
                bead_id: "b".into(),
                status: "open".into(),
            }
        }
        pub fn api_cost_session() -> ApiCostSession {
            ApiCostSession {
                session_id: "s".into(),
                agent_name: "a".into(),
                input_tokens: 1,
                output_tokens: 1,
            }
        }
        pub fn api_mcp_server() -> ApiMcpServer {
            ApiMcpServer {
                name: "fs".into(),
                status: "ready".into(),
                tools: vec!["read".into()],
            }
        }
        pub fn api_memory_entry() -> ApiMemoryEntry {
            ApiMemoryEntry {
                id: "m".into(),
                category: "c".into(),
                content: "x".into(),
                created_at: "0".into(),
            }
        }
        pub fn api_roadmap_feature() -> ApiRoadmapFeature {
            ApiRoadmapFeature {
                id: "rf".into(),
                title: "t".into(),
                description: "d".into(),
                status: "p".into(),
                priority: 1,
                estimated_effort: "1d".into(),
                dependencies: vec![],
                created_at: "0".into(),
            }
        }
        pub fn api_roadmap() -> ApiRoadmap {
            ApiRoadmap {
                id: "r".into(),
                name: "n".into(),
                features: vec![api_roadmap_feature()],
                generated_at: "0".into(),
            }
        }
        pub fn api_roadmap_item() -> ApiRoadmapItem {
            ApiRoadmapItem {
                id: "ri".into(),
                title: "t".into(),
                description: "d".into(),
                status: "o".into(),
                priority: "p".into(),
            }
        }
        pub fn api_idea() -> ApiIdea {
            ApiIdea {
                id: "i".into(),
                title: "t".into(),
                description: "d".into(),
                category: "c".into(),
                impact: "h".into(),
                effort: "l".into(),
            }
        }
        pub fn api_stack_node() -> ApiStackNode {
            ApiStackNode {
                id: "sn".into(),
                title: "t".into(),
                phase: "p".into(),
                git_branch: None,
                pr_number: None,
                stack_position: 0,
            }
        }
        pub fn api_stack() -> ApiStack {
            ApiStack {
                root: api_stack_node(),
                children: vec![],
                total: 1,
            }
        }
        pub fn api_github_issue() -> ApiGithubIssue {
            ApiGithubIssue {
                number: 1,
                title: "t".into(),
                labels: vec![],
                assignee: None,
                state: "open".into(),
                created: "0".into(),
                created_at: None,
            }
        }
        pub fn api_github_pr() -> ApiGithubPr {
            ApiGithubPr {
                number: 1,
                title: "t".into(),
                author: "a".into(),
                status: "open".into(),
                state: None,
                reviewers: vec![],
                created: "0".into(),
                created_at: None,
            }
        }
        pub fn api_changelog_section() -> ApiChangelogSection {
            ApiChangelogSection {
                category: "Added".into(),
                items: vec!["x".into()],
            }
        }
        pub fn api_changelog_entry() -> ApiChangelogEntry {
            ApiChangelogEntry {
                id: "cl".into(),
                version: "1.0.0".into(),
                date: "0".into(),
                sections: vec![api_changelog_section()],
            }
        }
        pub fn create_bead_request() -> CreateBeadRequest {
            CreateBeadRequest {
                title: "t".into(),
                description: None,
                lane: None,
            }
        }
        pub fn update_status_request() -> UpdateStatusRequest {
            UpdateStatusRequest {
                status: "done".into(),
            }
        }
        pub fn add_memory_request() -> AddMemoryRequest {
            AddMemoryRequest {
                key: "k".into(),
                value: "v".into(),
                category: "c".into(),
                source: "u".into(),
            }
        }
        pub fn send_insights_message_request() -> SendInsightsMessageRequest {
            SendInsightsMessageRequest {
                content: "h".into(),
            }
        }
        pub fn add_mcp_server_request() -> AddMcpServerRequest {
            AddMcpServerRequest {
                name: "n".into(),
                command: "/x".into(),
                args: None,
            }
        }
        pub fn review_gitlab_mr_request() -> ReviewGitLabMrRequest {
            ReviewGitLabMrRequest {
                project_id: None,
                strict: None,
                severity_threshold: None,
            }
        }
        pub fn create_task_request() -> CreateTaskRequest {
            CreateTaskRequest {
                title: "t".into(),
                description: None,
                bead_id: "b".into(),
                priority: "p1".into(),
                complexity: "M".into(),
                category: "c".into(),
            }
        }
        pub fn generate_changelog_request() -> GenerateChangelogRequest {
            GenerateChangelogRequest {
                commits: "0..1".into(),
                version: "1.0.0".into(),
            }
        }
        pub fn publish_github_release_request() -> PublishGithubReleaseRequest {
            PublishGithubReleaseRequest {
                tag_name: "v1".into(),
                name: "r".into(),
                body: "n".into(),
                draft: false,
                prerelease: false,
            }
        }
        pub fn add_roadmap_feature_request() -> AddRoadmapFeatureRequest {
            AddRoadmapFeatureRequest {
                title: "t".into(),
                description: "d".into(),
                status: "p".into(),
                priority: "p1".into(),
            }
        }
        pub fn send_insights_message_with_model_request() -> SendInsightsMessageWithModelRequest {
            SendInsightsMessageWithModelRequest {
                content: "h".into(),
                model: None,
            }
        }
        pub fn create_project_request() -> CreateProjectRequest {
            CreateProjectRequest {
                name: "p".into(),
                path: "/tmp/p".into(),
            }
        }
        pub fn update_project_request() -> UpdateProjectRequest {
            UpdateProjectRequest {
                name: None,
                path: None,
            }
        }
    }
}
