//! Shared API response types for auto-tundra services.
//!
//! This crate provides common type definitions used across multiple services
//! to ensure consistency in API responses and reduce code duplication.

use serde::{Deserialize, Serialize};

// ── Core API response types (matching backend JSON) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiCosts {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub sessions: Vec<ApiCostSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMcpServer {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiStack {
    pub root: ApiStackNode,
    #[serde(default)]
    pub children: Vec<ApiStackNode>,
    #[serde(default)]
    pub total: u32,
}

// ── GitHub API types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiChangelogSection {
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBeadRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lane: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStatusRequest {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMemoryRequest {
    pub key: String,
    pub value: String,
    pub category: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendInsightsMessageRequest {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMcpServerRequest {
    pub name: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewGitLabMrRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity_threshold: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub bead_id: String,
    pub priority: String,
    pub complexity: String,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateChangelogRequest {
    pub commits: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishGithubReleaseRequest {
    pub tag_name: String,
    pub name: String,
    pub body: String,
    pub draft: bool,
    pub prerelease: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddRoadmapFeatureRequest {
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendInsightsMessageWithModelRequest {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProjectRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}
