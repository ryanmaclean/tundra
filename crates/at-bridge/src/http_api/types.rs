use serde::{Deserialize, Serialize};
use uuid::Uuid;

use at_core::types::{
    AgentProfile, BeadStatus, CliType, Lane, PhaseConfig, TaskCategory, TaskComplexity, TaskImpact,
    TaskPhase, TaskPriority, TaskSource,
};

// ---------------------------------------------------------------------------
// Project / Sync / Attachment / Draft types
// ---------------------------------------------------------------------------

/// A project represents a workspace/repository that can be managed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub created_at: String,
    pub is_active: bool,
}

/// Sync status tracking for GitHub issue synchronization.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncStatus {
    pub last_sync_time: Option<chrono::DateTime<chrono::Utc>>,
    pub issues_imported: u64,
    pub issues_exported: u64,
    pub statuses_synced: u64,
    pub is_syncing: bool,
}

/// Metadata stub for an image/screenshot attachment on a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: Uuid,
    pub task_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub uploaded_at: String,
}

/// A saved task draft for auto-save functionality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDraft {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub category: Option<String>,
    pub priority: Option<String>,
    pub files: Vec<String>,
    pub updated_at: String,
}

/// Status of a watched pull request being polled in the background.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrPollStatus {
    pub pr_number: u32,
    pub state: String,
    pub mergeable: Option<bool>,
    pub checks_passed: Option<bool>,
    pub last_polled: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineQueueStatus {
    pub limit: usize,
    pub waiting: usize,
    pub running: usize,
    pub available_permits: usize,
}

// ---------------------------------------------------------------------------
// Kanban types
// ---------------------------------------------------------------------------

/// Default 8 Kanban columns (Backlog, Queue, In Progress, Review, QA, Done, PR Created, Error).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanColumnConfig {
    pub columns: Vec<KanbanColumn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanColumn {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_px: Option<u16>,
}

// ---------------------------------------------------------------------------
// Planning Poker types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningPokerVote {
    pub voter: String,
    pub card: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanningPokerPhase {
    Idle,
    Voting,
    Revealed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningPokerSession {
    pub bead_id: Uuid,
    pub phase: PlanningPokerPhase,
    pub votes: Vec<PlanningPokerVote>,
    pub participants: Vec<String>,
    pub deck: Vec<String>,
    pub round_duration_seconds: Option<u64>,
    pub consensus_card: Option<String>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Response payload for GET /api/status.
#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub version: String,
    pub uptime_seconds: u64,
    pub agent_count: usize,
    pub bead_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct CreateBeadRequest {
    pub title: String,
    pub description: Option<String>,
    pub lane: Option<Lane>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBeadStatusRequest {
    pub status: BeadStatus,
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub bead_id: Uuid,
    pub category: TaskCategory,
    pub priority: TaskPriority,
    pub complexity: TaskComplexity,
    pub description: Option<String>,
    pub impact: Option<TaskImpact>,
    pub agent_profile: Option<AgentProfile>,
    pub phase_configs: Option<Vec<PhaseConfig>>,
    pub source: Option<TaskSource>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub category: Option<TaskCategory>,
    pub priority: Option<TaskPriority>,
    pub complexity: Option<TaskComplexity>,
    pub impact: Option<TaskImpact>,
    pub agent_profile: Option<AgentProfile>,
    pub phase_configs: Option<Vec<PhaseConfig>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTaskPhaseRequest {
    pub phase: TaskPhase,
}

#[derive(Debug, Deserialize)]
pub struct NotificationQuery {
    pub unread: Option<bool>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct BeadQuery {
    pub status: Option<BeadStatus>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct AgentQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct ArchivedTaskQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct AttachmentQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct TaskDraftQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

// ---------------------------------------------------------------------------
// Merge / Queue / DirectMode request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ResolveConflictRequest {
    pub strategy: String,
    pub file: String,
}

#[derive(Debug, Deserialize)]
pub struct QueueReorderRequest {
    pub task_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct PrioritizeRequest {
    pub priority: TaskPriority,
}

#[derive(Debug, Deserialize)]
pub struct DirectModeRequest {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteTaskRequest {
    /// Optional CLI type override; defaults to Claude.
    pub cli_type: Option<CliType>,
}

/// Response entry for `GET /api/cli/available`.
#[derive(Debug, Serialize)]
pub struct CliAvailabilityEntry {
    pub name: String,
    pub detected: bool,
    pub path: Option<String>,
}

// ---------------------------------------------------------------------------
// Column lock / task ordering / file watch types
// ---------------------------------------------------------------------------

/// Column locking prevents drag-drop into/from a column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnLock {
    pub column_id: String,
    pub locked: bool,
}

#[derive(Debug, Deserialize)]
pub struct LockColumnRequest {
    pub column_id: String,
    pub locked: bool,
}

/// Persist task ordering within a kanban column.
#[derive(Debug, Deserialize)]
pub struct TaskOrderingRequest {
    pub column_id: String,
    pub task_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct StartPlanningPokerRequest {
    pub bead_id: Uuid,
    #[serde(default)]
    pub participants: Vec<String>,
    pub deck_preset: Option<String>,
    pub custom_deck: Option<Vec<String>>,
    pub round_duration_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitPlanningPokerVoteRequest {
    pub bead_id: Uuid,
    pub voter: String,
    pub card: String,
}

#[derive(Debug, Deserialize)]
pub struct RevealPlanningPokerRequest {
    pub bead_id: Uuid,
}

fn default_poker_auto_reveal() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct SimulatePlanningPokerRequest {
    pub bead_id: Uuid,
    #[serde(default)]
    pub virtual_agents: Vec<String>,
    pub agent_count: Option<usize>,
    pub deck_preset: Option<String>,
    pub custom_deck: Option<Vec<String>>,
    pub round_duration_seconds: Option<u64>,
    pub focus_card: Option<String>,
    pub seed: Option<u64>,
    #[serde(default = "default_poker_auto_reveal")]
    pub auto_reveal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningPokerVoteView {
    pub voter: String,
    pub has_voted: bool,
    pub card: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningPokerRevealStats {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub average: Option<f64>,
    pub median: Option<f64>,
    pub mode: Option<f64>,
    pub numeric_vote_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningPokerSessionResponse {
    pub bead_id: Uuid,
    pub phase: PlanningPokerPhase,
    pub revealed: bool,
    pub deck: Vec<String>,
    pub round_duration_seconds: Option<u64>,
    pub vote_count: usize,
    pub votes: Vec<PlanningPokerVoteView>,
    pub consensus_card: Option<String>,
    pub stats: Option<PlanningPokerRevealStats>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Request to start watching a file/directory for changes.
#[derive(Debug, Deserialize)]
pub struct FileWatchRequest {
    pub path: String,
    pub recursive: bool,
}

/// Competitor analysis input for the roadmap feature.
#[derive(Debug, Deserialize)]
pub struct CompetitorAnalysisRequest {
    pub competitor_name: String,
    pub competitor_url: Option<String>,
    pub focus_areas: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompetitorAnalysisResult {
    pub competitor_name: String,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
    pub opportunities: Vec<String>,
    pub analyzed_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Task list query
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TaskListQuery {
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

// ---------------------------------------------------------------------------
// Build log query
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
pub struct BuildLogsQuery {
    /// ISO-8601 timestamp; only return entries newer than this.
    #[serde(default)]
    pub since: Option<String>,
}

/// Summary of the current build status for a task.
#[derive(Debug, Serialize)]
pub struct BuildStatusSummary {
    pub phase: TaskPhase,
    pub progress_percent: u8,
    pub total_lines: usize,
    pub stdout_lines: usize,
    pub stderr_lines: usize,
    pub error_count: usize,
    pub last_line: Option<String>,
}

// ---------------------------------------------------------------------------
// GitHub types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub message: String,
    pub imported: u64,
    pub statuses_synced: u64,
}

#[derive(Debug, Serialize)]
pub struct PrCreatedResponse {
    pub message: String,
    pub task_id: Uuid,
    pub pr_title: String,
    pub pr_branch: Option<String>,
}

/// Request body for creating a PR (supports stacked PRs via base_branch).
#[derive(Debug, Default, serde::Deserialize)]
pub struct CreatePrRequest {
    #[serde(default)]
    pub base_branch: Option<String>,
}

/// Query params for GET /api/github/issues.
#[derive(Debug, Default, serde::Deserialize)]
pub struct ListGitHubIssuesQuery {
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub labels: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub per_page: Option<u8>,
}

// ---------------------------------------------------------------------------
// GitLab types
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
pub struct ListGitLabIssuesQuery {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub per_page: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListGitLabMrsQuery {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub per_page: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ReviewGitLabMrBody {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub severity_threshold: Option<at_integrations::gitlab::mr_review::MrReviewSeverity>,
    #[serde(default)]
    pub max_findings: Option<usize>,
    #[serde(default)]
    pub auto_approve: Option<bool>,
}

// ---------------------------------------------------------------------------
// Linear types
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
pub struct ListLinearIssuesQuery {
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ImportLinearBody {
    pub issue_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// MCP types
// ---------------------------------------------------------------------------

/// Represents a Model Context Protocol (MCP) server with its available tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub name: String,
    pub status: String,
    pub tools: Vec<String>,
}

// ---------------------------------------------------------------------------
// Cost types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostResponse {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub sessions: Vec<CostSessionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSessionEntry {
    pub session_id: String,
    pub agent_name: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

// ---------------------------------------------------------------------------
// Agent session types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSessionEntry {
    pub id: String,
    pub agent_name: String,
    pub cli_type: String,
    pub status: String,
    pub duration: String,
}

// ---------------------------------------------------------------------------
// Convoy types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvoyEntry {
    pub id: String,
    pub name: String,
    pub bead_count: u32,
    pub status: String,
}

// ---------------------------------------------------------------------------
// Project request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

// ---------------------------------------------------------------------------
// Release types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateReleaseRequest {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
}
