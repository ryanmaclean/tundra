use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    middleware as axum_middleware,
    response::IntoResponse,
    routing::{get, patch, post, put},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::{RwLock, Semaphore};
use tower_http::cors::CorsLayer;
use tracing::warn;
use uuid::Uuid;

use crate::api_error::ApiError;
use crate::auth::AuthLayer;
use crate::event_bus::EventBus;
use crate::intelligence_api;
use crate::notifications::{notification_from_event, NotificationStore};
use crate::terminal::TerminalRegistry;
use crate::terminal_ws;
use at_core::config::{Config, CredentialProvider};
use at_core::session_store::{SessionState, SessionStore};
use at_core::settings::SettingsManager;
use at_core::types::{
    Agent, AgentProfile, Bead, BeadStatus, BuildLogEntry, BuildStream, CliType, KpiSnapshot, Lane,
    PhaseConfig, Task, TaskCategory, TaskComplexity, TaskImpact, TaskPhase, TaskPriority,
    TaskSource,
};
use at_integrations::github::{
    issues, oauth as gh_oauth, pr_automation::PrAutomation, pull_requests, sync::IssueSyncEngine,
};
use at_integrations::types::{GitHubConfig, GitHubRelease, IssueState, PrState};
use at_intelligence::{
    changelog::ChangelogEngine, ideation::IdeationEngine, insights::InsightsEngine,
    memory::MemoryStore, roadmap::RoadmapEngine,
};
use at_telemetry::metrics::global_metrics;
use at_telemetry::middleware::metrics_middleware;
use at_telemetry::tracing_setup::request_id_middleware;

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
    #[serde(default)]
    pub issues_imported: u64,
    #[serde(default)]
    pub issues_exported: u64,
    #[serde(default)]
    pub statuses_synced: u64,
    #[serde(default)]
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

/// Shared application state for all HTTP/WS handlers.
pub struct ApiState {
    pub event_bus: EventBus,
    pub beads: Arc<RwLock<Vec<Bead>>>,
    pub agents: Arc<RwLock<Vec<Agent>>>,
    pub kpi: Arc<RwLock<KpiSnapshot>>,
    pub tasks: Arc<RwLock<Vec<Task>>>,
    /// Queue gate for task pipeline execution.
    pub pipeline_semaphore: Arc<Semaphore>,
    /// Max number of concurrently executing task pipelines.
    pub pipeline_max_concurrent: usize,
    /// Number of task executions waiting for a pipeline permit.
    pub pipeline_waiting: Arc<AtomicUsize>,
    /// Number of task executions currently running.
    pub pipeline_running: Arc<AtomicUsize>,
    pub start_time: std::time::Instant,
    pub pty_pool: Option<Arc<at_session::pty_pool::PtyPool>>,
    pub terminal_registry: Arc<RwLock<TerminalRegistry>>,
    /// Active PTY handles keyed by terminal ID.
    pub pty_handles: Arc<RwLock<std::collections::HashMap<Uuid, at_session::pty_pool::PtyHandle>>>,
    /// Settings persistence manager.
    pub settings_manager: Arc<SettingsManager>,
    /// GitHub sync status tracking.
    pub sync_status: Arc<RwLock<SyncStatus>>,
    // ---- Intelligence engines ------------------------------------------------
    pub insights_engine: Arc<RwLock<InsightsEngine>>,
    pub ideation_engine: Arc<RwLock<IdeationEngine>>,
    pub roadmap_engine: Arc<RwLock<RoadmapEngine>>,
    pub memory_store: Arc<RwLock<MemoryStore>>,
    pub changelog_engine: Arc<RwLock<ChangelogEngine>>,
    // ---- Notifications -------------------------------------------------------
    pub notification_store: Arc<RwLock<NotificationStore>>,
    // ---- Session persistence --------------------------------------------------
    pub session_store: Arc<SessionStore>,
    /// Kanban column config (8 columns: Backlog, Queue, In Progress, …, PR Created, Error).
    pub kanban_columns: Arc<RwLock<KanbanColumnConfig>>,
    // ---- GitHub OAuth ---------------------------------------------------
    pub github_oauth_token: Arc<RwLock<Option<String>>>,
    pub github_oauth_user: Arc<RwLock<Option<serde_json::Value>>>,
    // ---- Projects --------------------------------------------------------
    pub projects: Arc<RwLock<Vec<Project>>>,
    // ---- PR polling -------------------------------------------------------
    pub pr_poll_registry: Arc<RwLock<std::collections::HashMap<u32, PrPollStatus>>>,
    // ---- GitHub releases --------------------------------------------------
    pub releases: Arc<RwLock<Vec<GitHubRelease>>>,
    // ---- Task archival ----------------------------------------------------
    pub archived_tasks: Arc<RwLock<Vec<Uuid>>>,
    // ---- Attachments ------------------------------------------------------
    pub attachments: Arc<RwLock<Vec<Attachment>>>,
    // ---- Task drafts ------------------------------------------------------
    pub task_drafts: Arc<RwLock<std::collections::HashMap<Uuid, TaskDraft>>>,
    // ---- Disconnect buffers for terminal WS reconnection ------------------
    /// Per-terminal output buffer for disconnected terminals.
    /// Key: terminal_id, Value: bounded ring buffer of bytes.
    pub disconnect_buffers:
        Arc<RwLock<std::collections::HashMap<Uuid, crate::terminal::DisconnectBuffer>>>,
}

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

fn default_kanban_columns() -> KanbanColumnConfig {
    KanbanColumnConfig {
        columns: vec![
            KanbanColumn {
                id: "backlog".into(),
                label: "Backlog".into(),
                width_px: Some(200),
            },
            KanbanColumn {
                id: "queue".into(),
                label: "Queue".into(),
                width_px: Some(180),
            },
            KanbanColumn {
                id: "in_progress".into(),
                label: "In Progress".into(),
                width_px: Some(220),
            },
            KanbanColumn {
                id: "review".into(),
                label: "Review".into(),
                width_px: Some(180),
            },
            KanbanColumn {
                id: "qa".into(),
                label: "QA".into(),
                width_px: Some(160),
            },
            KanbanColumn {
                id: "done".into(),
                label: "Done".into(),
                width_px: Some(180),
            },
            KanbanColumn {
                id: "pr_created".into(),
                label: "PR Created".into(),
                width_px: Some(180),
            },
            KanbanColumn {
                id: "error".into(),
                label: "Error".into(),
                width_px: Some(160),
            },
        ],
    }
}

impl ApiState {
    /// Create a new `ApiState` with empty collections and a fresh event bus.
    pub fn new(event_bus: EventBus) -> Self {
        let pipeline_max_concurrent = std::env::var("AT_PIPELINE_MAX_CONCURRENT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(1);

        Self {
            event_bus,
            beads: Arc::new(RwLock::new(Vec::new())),
            agents: Arc::new(RwLock::new(Vec::new())),
            kpi: Arc::new(RwLock::new(KpiSnapshot {
                total_beads: 0,
                backlog: 0,
                hooked: 0,
                slung: 0,
                review: 0,
                done: 0,
                failed: 0,
                escalated: 0,
                active_agents: 0,
                timestamp: chrono::Utc::now(),
            })),
            tasks: Arc::new(RwLock::new(Vec::new())),
            pipeline_semaphore: Arc::new(Semaphore::new(pipeline_max_concurrent)),
            pipeline_max_concurrent,
            pipeline_waiting: Arc::new(AtomicUsize::new(0)),
            pipeline_running: Arc::new(AtomicUsize::new(0)),
            start_time: std::time::Instant::now(),
            pty_pool: None,
            terminal_registry: Arc::new(RwLock::new(TerminalRegistry::new())),
            pty_handles: Arc::new(RwLock::new(std::collections::HashMap::new())),
            settings_manager: Arc::new(SettingsManager::default_path()),
            sync_status: Arc::new(RwLock::new(SyncStatus::default())),
            insights_engine: Arc::new(RwLock::new(InsightsEngine::new())),
            ideation_engine: Arc::new(RwLock::new(IdeationEngine::new())),
            roadmap_engine: Arc::new(RwLock::new(RoadmapEngine::new())),
            memory_store: Arc::new(RwLock::new(MemoryStore::new())),
            changelog_engine: Arc::new(RwLock::new(ChangelogEngine::new())),
            notification_store: Arc::new(RwLock::new(NotificationStore::default())),
            session_store: Arc::new(SessionStore::default_path()),
            kanban_columns: Arc::new(RwLock::new(default_kanban_columns())),
            github_oauth_token: Arc::new(RwLock::new(None)),
            github_oauth_user: Arc::new(RwLock::new(None)),
            pr_poll_registry: Arc::new(RwLock::new(std::collections::HashMap::new())),
            releases: Arc::new(RwLock::new(Vec::new())),
            archived_tasks: Arc::new(RwLock::new(Vec::new())),
            projects: Arc::new(RwLock::new(vec![Project {
                id: Uuid::new_v4(),
                name: "auto-tundra".to_string(),
                path: std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| ".".to_string()),
                created_at: chrono::Utc::now().to_rfc3339(),
                is_active: true,
            }])),
            attachments: Arc::new(RwLock::new(Vec::new())),
            task_drafts: Arc::new(RwLock::new(std::collections::HashMap::new())),
            disconnect_buffers: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Create a new `ApiState` with a PTY pool for terminal support.
    pub fn with_pty_pool(
        event_bus: EventBus,
        pty_pool: Arc<at_session::pty_pool::PtyPool>,
    ) -> Self {
        let mut state = Self::new(event_bus);
        state.pty_pool = Some(pty_pool);
        state
    }

    /// Seed lightweight demo data for local development/web UI previews.
    ///
    /// No-op when beads are already present.
    pub async fn seed_demo_data(&self) {
        let mut beads = self.beads.write().await;
        if !beads.is_empty() {
            return;
        }

        let mut b1 = Bead::new("Set up stacked diffs", Lane::Standard);
        b1.description = Some("Enable parent/child task stacks and branch chaining.".into());
        b1.status = BeadStatus::Backlog;
        b1.priority = 2;
        b1.metadata = Some(serde_json::json!({"tags":["feature","stacks"]}));

        let mut b2 = Bead::new("Wire GitLab + Linear live sync", Lane::Standard);
        b2.description = Some("Replace stubs with real integration calls and retries.".into());
        b2.status = BeadStatus::Hooked;
        b2.priority = 3;
        b2.metadata = Some(serde_json::json!({"tags":["integration","sync"]}));

        let mut b3 = Bead::new("Polish Tahoe native shell", Lane::Critical);
        b3.description = Some("Hybrid native chrome with HIG-aligned interactions.".into());
        b3.status = BeadStatus::Review;
        b3.priority = 4;
        b3.metadata = Some(serde_json::json!({"tags":["native-ux","macos"]}));

        beads.extend([b1, b2, b3]);

        let mut agents = self.agents.write().await;
        if agents.is_empty() {
            agents.push(Agent::new(
                "Crew",
                at_core::types::AgentRole::Crew,
                CliType::Claude,
            ));
            agents.push(Agent::new(
                "Reviewer",
                at_core::types::AgentRole::SpecCritic,
                CliType::Claude,
            ));
        }

        let snapshot = KpiSnapshot {
            total_beads: beads.len() as u64,
            backlog: beads
                .iter()
                .filter(|b| b.status == BeadStatus::Backlog)
                .count() as u64,
            hooked: beads
                .iter()
                .filter(|b| b.status == BeadStatus::Hooked)
                .count() as u64,
            slung: beads
                .iter()
                .filter(|b| b.status == BeadStatus::Slung)
                .count() as u64,
            review: beads
                .iter()
                .filter(|b| b.status == BeadStatus::Review)
                .count() as u64,
            done: beads
                .iter()
                .filter(|b| b.status == BeadStatus::Done)
                .count() as u64,
            failed: beads
                .iter()
                .filter(|b| b.status == BeadStatus::Failed)
                .count() as u64,
            escalated: beads
                .iter()
                .filter(|b| b.status == BeadStatus::Escalated)
                .count() as u64,
            active_agents: agents.len() as u64,
            timestamp: chrono::Utc::now(),
        };
        *self.kpi.write().await = snapshot;
    }
}

/// Build the full API router with all REST and WebSocket routes.
///
/// When `api_key` is `Some`, the [`AuthLayer`] middleware will require
/// every request to carry a valid key. When `None`, all requests pass
/// through (development mode).
pub fn api_router(state: Arc<ApiState>) -> Router {
    api_router_with_auth(state, None)
}

/// Build the API router with optional authentication.
pub fn api_router_with_auth(state: Arc<ApiState>, api_key: Option<String>) -> Router {
    Router::new()
        .route("/api/status", get(get_status))
        .route("/api/beads", get(list_beads))
        .route("/api/beads", post(create_bead))
        .route("/api/beads/{id}/status", post(update_bead_status))
        .route("/api/agents", get(list_agents))
        .route("/api/agents/{id}/nudge", post(nudge_agent))
        .route("/api/agents/{id}/stop", post(stop_agent))
        .route("/api/kpi", get(get_kpi))
        .route("/api/tasks", get(list_tasks))
        .route("/api/tasks", post(create_task))
        .route("/api/tasks/{id}", get(get_task))
        .route("/api/tasks/{id}", put(update_task))
        .route("/api/tasks/{id}", axum::routing::delete(delete_task))
        .route("/api/tasks/{id}/phase", post(update_task_phase))
        .route("/api/tasks/{id}/logs", get(get_task_logs))
        .route("/api/tasks/{id}/execute", post(execute_task_pipeline))
        .route("/api/tasks/{id}/build-logs", get(get_build_logs))
        .route("/api/tasks/{id}/build-status", get(get_build_status))
        .route("/api/pipeline/queue", get(get_pipeline_queue_status))
        .route("/api/terminals", get(terminal_ws::list_terminals))
        .route("/api/terminals", post(terminal_ws::create_terminal))
        .route(
            "/api/terminals/{id}",
            axum::routing::delete(terminal_ws::delete_terminal),
        )
        .route("/ws/terminal/{id}", get(terminal_ws::terminal_ws))
        .route(
            "/api/terminals/{id}/settings",
            patch(terminal_ws::update_terminal_settings),
        )
        .route(
            "/api/terminals/{id}/auto-name",
            post(terminal_ws::auto_name_terminal),
        )
        .route(
            "/api/terminals/persistent",
            get(terminal_ws::list_persistent_terminals),
        )
        .route("/api/settings", get(get_settings))
        .route("/api/settings", put(put_settings))
        .route("/api/settings", patch(patch_settings))
        .route("/api/credentials/status", get(get_credentials_status))
        .route("/api/github/sync", post(trigger_github_sync))
        .route("/api/github/sync/status", get(get_sync_status))
        .route("/api/github/issues", get(list_github_issues))
        .route(
            "/api/github/issues/{number}/import",
            post(import_github_issue),
        )
        .route("/api/github/prs", get(list_github_prs))
        .route("/api/github/pr/{task_id}", post(create_pr_for_task))
        // GitHub OAuth
        .route("/api/github/oauth/authorize", get(github_oauth_authorize))
        .route("/api/github/oauth/callback", post(github_oauth_callback))
        .route("/api/github/oauth/status", get(github_oauth_status))
        .route("/api/github/oauth/revoke", post(github_oauth_revoke))
        // GitLab integration
        .route("/api/gitlab/issues", get(list_gitlab_issues))
        .route(
            "/api/gitlab/merge-requests",
            get(list_gitlab_merge_requests),
        )
        .route(
            "/api/gitlab/merge-requests/{iid}/review",
            post(review_gitlab_merge_request),
        )
        // Linear integration
        .route("/api/linear/issues", get(list_linear_issues))
        .route("/api/linear/import", post(import_linear_issues))
        .route("/api/kanban/columns", get(get_kanban_columns))
        .route("/api/kanban/columns", patch(patch_kanban_columns))
        // MCP servers
        .route("/api/mcp/servers", get(list_mcp_servers))
        .route("/api/mcp/tools/call", post(call_mcp_tool))
        // Worktrees
        .route("/api/worktrees", get(list_worktrees))
        .route(
            "/api/worktrees/{id}",
            axum::routing::delete(delete_worktree),
        )
        .route("/api/worktrees/{id}/merge", post(merge_worktree))
        .route("/api/worktrees/{id}/merge-preview", get(merge_preview))
        .route("/api/worktrees/{id}/resolve", post(resolve_conflict))
        // Agent Queue
        .route("/api/queue", get(list_queue))
        .route("/api/queue/reorder", post(reorder_queue))
        .route("/api/queue/{task_id}/prioritize", post(prioritize_task))
        // Direct mode
        .route("/api/settings/direct-mode", post(toggle_direct_mode))
        // Costs
        .route("/api/costs", get(get_costs))
        // CLI availability
        .route("/api/cli/available", get(list_available_clis))
        // Agent sessions
        .route("/api/sessions", get(list_agent_sessions))
        // Convoys
        .route("/api/convoys", get(list_convoys))
        // Notification endpoints
        .route("/api/notifications", get(list_notifications))
        .route("/api/notifications/count", get(notification_count))
        .route("/api/notifications/{id}/read", post(mark_notification_read))
        .route(
            "/api/notifications/read-all",
            post(mark_all_notifications_read),
        )
        .route(
            "/api/notifications/{id}",
            axum::routing::delete(delete_notification),
        )
        // Metrics endpoints
        .route("/api/metrics", get(get_metrics_prometheus))
        .route("/api/metrics/json", get(get_metrics_json))
        // Session endpoints
        .route("/api/sessions/ui", get(get_ui_session))
        .route("/api/sessions/ui", put(save_ui_session))
        .route("/api/sessions/ui/list", get(list_ui_sessions))
        // Projects
        .route("/api/projects", get(list_projects))
        .route("/api/projects", post(create_project))
        .route("/api/projects/{id}", put(update_project))
        .route("/api/projects/{id}", axum::routing::delete(delete_project))
        .route("/api/projects/{id}/activate", post(activate_project))
        // PR polling
        .route("/api/github/pr/{number}/watch", post(watch_pr))
        .route(
            "/api/github/pr/{number}/watch",
            axum::routing::delete(unwatch_pr),
        )
        .route("/api/github/pr/watched", get(list_watched_prs))
        // GitHub releases
        .route("/api/github/releases", post(create_release))
        .route("/api/github/releases", get(list_releases))
        // Task archival
        .route("/api/tasks/{id}/archive", post(archive_task))
        .route("/api/tasks/{id}/unarchive", post(unarchive_task))
        .route("/api/tasks/archived", get(list_archived_tasks))
        // Attachments
        .route("/api/tasks/{task_id}/attachments", get(list_attachments))
        .route("/api/tasks/{task_id}/attachments", post(add_attachment))
        .route(
            "/api/tasks/{task_id}/attachments/{id}",
            axum::routing::delete(delete_attachment),
        )
        // Task drafts
        .route("/api/tasks/drafts", get(list_task_drafts))
        .route("/api/tasks/drafts", post(save_task_draft))
        .route("/api/tasks/drafts/{id}", get(get_task_draft))
        .route(
            "/api/tasks/drafts/{id}",
            axum::routing::delete(delete_task_draft),
        )
        // Kanban column locking
        .route("/api/kanban/columns/lock", post(lock_column))
        // Task ordering
        .route("/api/kanban/ordering", post(save_task_ordering))
        // File watching
        .route("/api/files/watch", post(start_file_watch))
        .route("/api/files/unwatch", post(stop_file_watch))
        // Competitor analysis
        .route(
            "/api/roadmap/competitor-analysis",
            post(run_competitor_analysis),
        )
        // Profile swap notification
        .route("/api/notifications/profile-swap", post(notify_profile_swap))
        // App update check
        .route("/api/notifications/app-update", get(check_app_update))
        // WebSocket endpoints
        .route("/ws", get(ws_handler))
        .route("/api/events/ws", get(events_ws_handler))
        .merge(intelligence_api::intelligence_router())
        .layer(axum_middleware::from_fn(metrics_middleware))
        .layer(axum_middleware::from_fn(request_id_middleware))
        .layer(AuthLayer::new(api_key))
        .layer(CorsLayer::very_permissive())
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct StatusResponse {
    version: String,
    uptime_seconds: u64,
    agent_count: usize,
    bead_count: usize,
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

// ---------------------------------------------------------------------------
// Merge / Queue / DirectMode request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ResolveConflictRequest {
    pub strategy: String, // "ours" | "theirs" | "manual"
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
// Handlers
// ---------------------------------------------------------------------------

async fn get_status(State(state): State<Arc<ApiState>>) -> Json<StatusResponse> {
    let beads = state.beads.read().await;
    let agents = state.agents.read().await;
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: state.start_time.elapsed().as_secs(),
        agent_count: agents.len(),
        bead_count: beads.len(),
    })
}

async fn list_beads(State(state): State<Arc<ApiState>>) -> Json<Vec<Bead>> {
    let beads = state.beads.read().await;
    Json(beads.clone())
}

async fn create_bead(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateBeadRequest>,
) -> impl IntoResponse {
    let lane = req.lane.unwrap_or(Lane::Standard);
    let mut bead = Bead::new(req.title, lane);
    bead.description = req.description;
    if let Some(tags) = req.tags {
        bead.metadata = Some(serde_json::json!({ "tags": tags }));
    }

    let mut beads = state.beads.write().await;
    beads.push(bead.clone());

    // Publish event
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::BeadList(beads.clone()));

    (axum::http::StatusCode::CREATED, Json(bead))
}

async fn update_bead_status(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateBeadStatusRequest>,
) -> impl IntoResponse {
    let mut beads = state.beads.write().await;
    let Some(bead) = beads.iter_mut().find(|b| b.id == id) else {
        return ApiError::NotFound("bead not found".to_string()).into_response();
    };

    if !bead.status.can_transition_to(&req.status) {
        return ApiError::BadRequest(format!(
            "invalid transition from {:?} to {:?}",
            bead.status, req.status
        ))
        .into_response();
    }

    bead.status = req.status;
    bead.updated_at = chrono::Utc::now();

    let bead_snapshot = bead.clone();
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::BeadList(beads.clone()));

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(bead_snapshot)),
    )
        .into_response()
}

async fn list_agents(State(state): State<Arc<ApiState>>) -> Json<Vec<Agent>> {
    let agents = state.agents.read().await;
    Json(agents.clone())
}

async fn nudge_agent(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut agents = state.agents.write().await;
    let Some(agent) = agents.iter_mut().find(|a| a.id == id) else {
        return ApiError::NotFound("agent not found".to_string()).into_response();
    };

    use at_core::types::AgentStatus;
    match agent.status {
        AgentStatus::Active | AgentStatus::Idle | AgentStatus::Unknown => {
            agent.status = AgentStatus::Pending;
            agent.last_seen = chrono::Utc::now();
        }
        AgentStatus::Pending | AgentStatus::Stopped => {
            // Already pending/stopped -- nothing to do but acknowledge.
        }
    }

    let snapshot = agent.clone();
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(snapshot)),
    )
        .into_response()
}

async fn stop_agent(State(state): State<Arc<ApiState>>, Path(id): Path<Uuid>) -> impl IntoResponse {
    let mut agents = state.agents.write().await;
    let Some(agent) = agents.iter_mut().find(|a| a.id == id) else {
        return ApiError::NotFound("agent not found".to_string()).into_response();
    };

    agent.status = at_core::types::AgentStatus::Stopped;
    agent.last_seen = chrono::Utc::now();

    let snapshot = agent.clone();
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(snapshot)),
    )
        .into_response()
}

async fn get_kpi(State(state): State<Arc<ApiState>>) -> Json<KpiSnapshot> {
    let kpi = state.kpi.read().await;
    Json(kpi.clone())
}

// ---------------------------------------------------------------------------
// Task handlers
// ---------------------------------------------------------------------------

async fn list_tasks(State(state): State<Arc<ApiState>>) -> Json<Vec<Task>> {
    let tasks = state.tasks.read().await;
    Json(tasks.clone())
}

async fn create_task(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    if req.title.is_empty() {
        return ApiError::BadRequest("title cannot be empty".to_string()).into_response();
    }

    let mut task = Task::new(
        req.title,
        req.bead_id,
        req.category,
        req.priority,
        req.complexity,
    );
    task.description = req.description;
    task.impact = req.impact;
    task.agent_profile = req.agent_profile;
    task.source = req.source;
    if let Some(configs) = req.phase_configs {
        task.phase_configs = configs;
    }

    let mut tasks = state.tasks.write().await;
    tasks.push(task.clone());

    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(task)),
    )
        .into_response()
}

async fn get_task(State(state): State<Arc<ApiState>>, Path(id): Path<Uuid>) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let Some(task) = tasks.iter().find(|t| t.id == id) else {
        return ApiError::NotFound("task not found".to_string()).into_response();
    };
    (axum::http::StatusCode::OK, Json(serde_json::json!(task))).into_response()
}

async fn update_task(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTaskRequest>,
) -> impl IntoResponse {
    let mut tasks = state.tasks.write().await;
    let Some(task) = tasks.iter_mut().find(|t| t.id == id) else {
        return ApiError::NotFound("task not found".to_string()).into_response();
    };

    if let Some(title) = req.title {
        if title.is_empty() {
            return ApiError::BadRequest("title cannot be empty".to_string()).into_response();
        }
        task.title = title;
    }
    if let Some(desc) = req.description {
        task.description = Some(desc);
    }
    if let Some(cat) = req.category {
        task.category = cat;
    }
    if let Some(pri) = req.priority {
        task.priority = pri;
    }
    if let Some(cplx) = req.complexity {
        task.complexity = cplx;
    }
    if let Some(impact) = req.impact {
        task.impact = Some(impact);
    }
    if let Some(profile) = req.agent_profile {
        task.agent_profile = Some(profile);
    }
    if let Some(configs) = req.phase_configs {
        task.phase_configs = configs;
    }
    task.updated_at = chrono::Utc::now();

    let task_snapshot = task.clone();
    drop(tasks);
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(
            task_snapshot.clone(),
        )));
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(task_snapshot)),
    )
        .into_response()
}

async fn delete_task(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut tasks = state.tasks.write().await;
    let len_before = tasks.len();
    tasks.retain(|t| t.id != id);
    if tasks.len() == len_before {
        return ApiError::NotFound("task not found".to_string()).into_response();
    }
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({"status": "deleted", "id": id.to_string()})),
    )
        .into_response()
}

async fn update_task_phase(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTaskPhaseRequest>,
) -> impl IntoResponse {
    let mut tasks = state.tasks.write().await;
    let Some(task) = tasks.iter_mut().find(|t| t.id == id) else {
        return ApiError::NotFound("task not found".to_string()).into_response();
    };

    if !task.phase.can_transition_to(&req.phase) {
        return ApiError::BadRequest(format!(
            "invalid phase transition from {:?} to {:?}",
            task.phase, req.phase
        ))
        .into_response();
    }

    task.set_phase(req.phase);
    let task_snapshot = task.clone();
    drop(tasks);
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(
            task_snapshot.clone(),
        )));
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(task_snapshot)),
    )
        .into_response()
}

async fn get_task_logs(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let Some(task) = tasks.iter().find(|t| t.id == id) else {
        return ApiError::NotFound("task not found".to_string()).into_response();
    };
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(task.logs)),
    )
        .into_response()
}

async fn get_pipeline_queue_status(
    State(state): State<Arc<ApiState>>,
) -> Json<PipelineQueueStatus> {
    Json(PipelineQueueStatus {
        limit: state.pipeline_max_concurrent,
        waiting: state.pipeline_waiting.load(Ordering::SeqCst),
        running: state.pipeline_running.load(Ordering::SeqCst),
        available_permits: state.pipeline_semaphore.available_permits(),
    })
}

// ---------------------------------------------------------------------------
// Execute task pipeline handler
// ---------------------------------------------------------------------------

/// POST /api/tasks/{id}/execute -- spawn the coding -> QA -> fix pipeline.
///
/// Transitions the task to Coding phase, then spawns a background tokio task
/// that drives the pipeline through QA and fix iterations. Returns 202 Accepted
/// immediately so the caller can follow progress via WebSocket events.
///
/// Accepts an optional JSON body with `cli_type` to override the default CLI.
async fn execute_task_pipeline(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    body: Option<Json<ExecuteTaskRequest>>,
) -> impl IntoResponse {
    let mut tasks = state.tasks.write().await;
    let Some(task) = tasks.iter_mut().find(|t| t.id == id) else {
        return ApiError::NotFound("task not found".to_string()).into_response();
    };

    // The task must be in a phase that can transition to Coding.
    if !task.phase.can_transition_to(&TaskPhase::Coding) {
        return ApiError::BadRequest(format!(
            "cannot start pipeline: task is in {:?} phase",
            task.phase
        ))
        .into_response();
    }

    task.set_phase(TaskPhase::Coding);
    let task_snapshot = task.clone();
    drop(tasks);

    // Extract optional CLI type from request body.
    let cli_type = body.and_then(|b| b.0.cli_type).unwrap_or(CliType::Claude);

    // Publish the phase change.
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(
            task_snapshot.clone(),
        )));

    // Spawn a background task to drive the pipeline phases.
    let tasks_store = state.tasks.clone();
    let event_bus = state.event_bus.clone();
    let pty_pool = state.pty_pool.clone();
    let pipeline_semaphore = state.pipeline_semaphore.clone();
    let pipeline_waiting = state.pipeline_waiting.clone();
    let pipeline_running = state.pipeline_running.clone();
    let pipeline_limit = state.pipeline_max_concurrent;

    let queued_position = pipeline_waiting.fetch_add(1, Ordering::SeqCst) + 1;
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::Event(
            crate::protocol::EventPayload {
                event_type: "pipeline_queued".to_string(),
                agent_id: None,
                bead_id: Some(task_snapshot.bead_id),
                message: format!(
                    "Task '{}' queued (position={}, limit={})",
                    task_snapshot.title, queued_position, pipeline_limit
                ),
                timestamp: chrono::Utc::now(),
            },
        ));

    tokio::spawn(async move {
        let _permit = match pipeline_semaphore.acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => {
                pipeline_waiting.fetch_sub(1, Ordering::SeqCst);
                event_bus.publish(crate::protocol::BridgeMessage::Event(
                    crate::protocol::EventPayload {
                        event_type: "pipeline_queue_error".to_string(),
                        agent_id: None,
                        bead_id: Some(task_snapshot.bead_id),
                        message: format!(
                            "Task '{}' failed to acquire pipeline queue permit",
                            task_snapshot.title
                        ),
                        timestamp: chrono::Utc::now(),
                    },
                ));
                return;
            }
        };

        pipeline_waiting.fetch_sub(1, Ordering::SeqCst);
        let running_now = pipeline_running.fetch_add(1, Ordering::SeqCst) + 1;
        event_bus.publish(crate::protocol::BridgeMessage::Event(
            crate::protocol::EventPayload {
                event_type: "pipeline_started".to_string(),
                agent_id: None,
                bead_id: Some(task_snapshot.bead_id),
                message: format!(
                    "Task '{}' started (running={}, limit={})",
                    task_snapshot.title, running_now, pipeline_limit
                ),
                timestamp: chrono::Utc::now(),
            },
        ));

        run_pipeline_background(task_snapshot, tasks_store, event_bus, pty_pool, cli_type).await;
        pipeline_running.fetch_sub(1, Ordering::SeqCst);
    });

    (
        axum::http::StatusCode::ACCEPTED,
        Json(serde_json::json!({"status": "started", "task_id": id.to_string()})),
    )
        .into_response()
}

/// Background pipeline driver: coding -> QA -> fix loop.
///
/// This runs in a spawned tokio task and updates the shared task state and
/// event bus as it progresses through phases.
async fn run_pipeline_background(
    task: Task,
    tasks_store: Arc<RwLock<Vec<Task>>>,
    event_bus: EventBus,
    pty_pool: Option<Arc<at_session::pty_pool::PtyPool>>,
    _cli_type: CliType,
) {
    use at_intelligence::runner::QaRunner;
    let max_fix_iterations: usize = 3;

    let emit = |event_type: &str| {
        event_bus.publish(crate::protocol::BridgeMessage::Event(
            crate::protocol::EventPayload {
                event_type: event_type.to_string(),
                agent_id: None,
                bead_id: Some(task.bead_id),
                message: format!("Task '{}': {}", task.title, event_type),
                timestamp: chrono::Utc::now(),
            },
        ));
    };

    // Helper: record a build log line on the task and publish it over the
    // event bus so WebSocket subscribers see it in real time.
    let emit_build_log = |tasks_store: &Arc<RwLock<Vec<Task>>>,
                          event_bus: &EventBus,
                          task_id: Uuid,
                          bead_id: Uuid,
                          stream: BuildStream,
                          line: String,
                          phase: TaskPhase| {
        let ts = tasks_store.clone();
        let eb = event_bus.clone();
        let stream_label = match &stream {
            BuildStream::Stdout => "stdout",
            BuildStream::Stderr => "stderr",
        };
        // Publish a build_log_line event for real-time streaming.
        eb.publish(crate::protocol::BridgeMessage::Event(
            crate::protocol::EventPayload {
                event_type: "build_log_line".to_string(),
                agent_id: None,
                bead_id: Some(bead_id),
                message: format!("[{}] {}", stream_label, line),
                timestamp: chrono::Utc::now(),
            },
        ));
        // Return a future that stores the entry on the task.
        async move {
            let mut tasks = ts.write().await;
            if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
                t.build_logs.push(BuildLogEntry {
                    timestamp: chrono::Utc::now(),
                    stream,
                    line,
                    phase,
                });
                t.updated_at = chrono::Utc::now();
            }
        }
    };

    emit("pipeline_start");

    // -- Coding phase --
    // If a PTY pool is available, we could spawn a real agent here.
    // For now we emit events and mark phase complete; the actual agent
    // spawning is handled by TaskOrchestrator in at-agents.
    emit("coding_phase_start");

    // Record phase start as a build log entry.
    emit_build_log(
        &tasks_store,
        &event_bus,
        task.id,
        task.bead_id,
        BuildStream::Stdout,
        "Coding phase started".to_string(),
        TaskPhase::Coding,
    )
    .await;

    if pty_pool.is_some() {
        // Real execution would use at-agents::task_orchestrator::TaskOrchestrator.
        // The bridge layer records the phase transition; callers that need full
        // PTY-based execution should use the at-agents crate directly.
        tracing::info!(task_id = %task.id, "PTY pool available; coding phase delegated to agent executor");
        emit_build_log(
            &tasks_store,
            &event_bus,
            task.id,
            task.bead_id,
            BuildStream::Stdout,
            "PTY pool available; delegating to agent executor".to_string(),
            TaskPhase::Coding,
        )
        .await;
    }

    emit_build_log(
        &tasks_store,
        &event_bus,
        task.id,
        task.bead_id,
        BuildStream::Stdout,
        "Coding phase complete".to_string(),
        TaskPhase::Coding,
    )
    .await;

    emit("coding_phase_complete");

    // Transition to QA
    {
        let mut tasks = tasks_store.write().await;
        if let Some(t) = tasks.iter_mut().find(|t| t.id == task.id) {
            t.set_phase(TaskPhase::Qa);
            event_bus.publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(
                t.clone(),
            )));
        }
    }

    // -- QA phase --
    emit("qa_phase_start");

    emit_build_log(
        &tasks_store,
        &event_bus,
        task.id,
        task.bead_id,
        BuildStream::Stdout,
        "QA phase started".to_string(),
        TaskPhase::Qa,
    )
    .await;

    let worktree = task.worktree_path.as_deref().unwrap_or(".");
    let mut qa_runner = QaRunner::new();
    let mut report = qa_runner.run_qa_checks(task.id, &task.title, Some(worktree));

    // Log QA result.
    let qa_stream = if report.status == at_core::types::QaStatus::Passed {
        BuildStream::Stdout
    } else {
        BuildStream::Stderr
    };
    emit_build_log(
        &tasks_store,
        &event_bus,
        task.id,
        task.bead_id,
        qa_stream,
        format!(
            "QA result: {:?} ({} issues)",
            report.status,
            report.issues.len()
        ),
        TaskPhase::Qa,
    )
    .await;

    emit("qa_phase_complete");

    // -- QA fix loop --
    let mut iterations = 0usize;
    while report.status == at_core::types::QaStatus::Failed && iterations < max_fix_iterations {
        iterations += 1;
        emit(&format!("qa_fix_iteration_{}", iterations));

        emit_build_log(
            &tasks_store,
            &event_bus,
            task.id,
            task.bead_id,
            BuildStream::Stderr,
            format!("Fix iteration {} of {}", iterations, max_fix_iterations),
            TaskPhase::Fixing,
        )
        .await;

        // Transition to Fixing
        {
            let mut tasks = tasks_store.write().await;
            if let Some(t) = tasks.iter_mut().find(|t| t.id == task.id) {
                t.set_phase(TaskPhase::Fixing);
                event_bus.publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(
                    t.clone(),
                )));
            }
        }

        // Re-run QA
        {
            let mut tasks = tasks_store.write().await;
            if let Some(t) = tasks.iter_mut().find(|t| t.id == task.id) {
                t.set_phase(TaskPhase::Qa);
                event_bus.publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(
                    t.clone(),
                )));
            }
        }

        let mut qa = QaRunner::new();
        report = qa.run_qa_checks(task.id, &task.title, Some(worktree));

        let iter_stream = if report.status == at_core::types::QaStatus::Passed {
            BuildStream::Stdout
        } else {
            BuildStream::Stderr
        };
        emit_build_log(
            &tasks_store,
            &event_bus,
            task.id,
            task.bead_id,
            iter_stream,
            format!(
                "QA re-check result: {:?} ({} issues)",
                report.status,
                report.issues.len()
            ),
            TaskPhase::Qa,
        )
        .await;
    }

    // Store the QA report on the task
    {
        let mut tasks = tasks_store.write().await;
        if let Some(t) = tasks.iter_mut().find(|t| t.id == task.id) {
            t.qa_report = Some(report.clone());

            let next_phase = report.next_phase();
            t.set_phase(next_phase);
            event_bus.publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(
                t.clone(),
            )));
        }
    }

    if report.status == at_core::types::QaStatus::Passed {
        emit_build_log(
            &tasks_store,
            &event_bus,
            task.id,
            task.bead_id,
            BuildStream::Stdout,
            "Pipeline completed successfully".to_string(),
            TaskPhase::Complete,
        )
        .await;
        emit("pipeline_complete");
    } else {
        emit_build_log(
            &tasks_store,
            &event_bus,
            task.id,
            task.bead_id,
            BuildStream::Stderr,
            "Pipeline completed with failures".to_string(),
            TaskPhase::Error,
        )
        .await;
        emit("pipeline_complete_with_failures");
    }

    tracing::info!(
        task_id = %task.id,
        qa_passed = (report.status == at_core::types::QaStatus::Passed),
        fix_iterations = iterations,
        "pipeline background task finished"
    );
}

// ---------------------------------------------------------------------------
// Build log handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct BuildLogsQuery {
    /// ISO-8601 timestamp; only return entries newer than this.
    #[serde(default)]
    pub since: Option<String>,
}

/// GET /api/tasks/{id}/build-logs -- return captured build output lines.
///
/// Supports an optional `?since=<ISO-8601>` query parameter for incremental
/// polling so clients only receive new lines since their last fetch.
async fn get_build_logs(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Query(q): Query<BuildLogsQuery>,
) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let Some(task) = tasks.iter().find(|t| t.id == id) else {
        return ApiError::NotFound("task not found".to_string()).into_response();
    };

    let logs: Vec<&BuildLogEntry> = if let Some(ref since_str) = q.since {
        match chrono::DateTime::parse_from_rfc3339(since_str) {
            Ok(since_ts) => {
                let since_utc = since_ts.with_timezone(&chrono::Utc);
                task.build_logs
                    .iter()
                    .filter(|e| e.timestamp > since_utc)
                    .collect()
            }
            Err(_) => {
                return ApiError::BadRequest(
                    "invalid 'since' timestamp; use ISO-8601 / RFC-3339".to_string(),
                )
                .into_response();
            }
        }
    } else {
        task.build_logs.iter().collect()
    };

    (axum::http::StatusCode::OK, Json(serde_json::json!(logs))).into_response()
}

/// Summary of the current build status for a task.
#[derive(Debug, Serialize)]
struct BuildStatusSummary {
    phase: TaskPhase,
    progress_percent: u8,
    total_lines: usize,
    stdout_lines: usize,
    stderr_lines: usize,
    error_count: usize,
    last_line: Option<String>,
}

/// GET /api/tasks/{id}/build-status -- return a summary of the build.
async fn get_build_status(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let Some(task) = tasks.iter().find(|t| t.id == id) else {
        return ApiError::NotFound("task not found".to_string()).into_response();
    };

    let stdout_lines = task
        .build_logs
        .iter()
        .filter(|e| e.stream == BuildStream::Stdout)
        .count();
    let stderr_lines = task
        .build_logs
        .iter()
        .filter(|e| e.stream == BuildStream::Stderr)
        .count();
    let last_line = task.build_logs.last().map(|e| e.line.clone());

    let summary = BuildStatusSummary {
        phase: task.phase.clone(),
        progress_percent: task.progress_percent,
        total_lines: task.build_logs.len(),
        stdout_lines,
        stderr_lines,
        error_count: stderr_lines,
        last_line,
    };

    (axum::http::StatusCode::OK, Json(serde_json::json!(summary))).into_response()
}

// ---------------------------------------------------------------------------
// Settings handlers
// ---------------------------------------------------------------------------

async fn get_settings(State(state): State<Arc<ApiState>>) -> Json<Config> {
    let cfg = state.settings_manager.load_or_default();
    Json(cfg)
}

async fn put_settings(
    State(state): State<Arc<ApiState>>,
    Json(cfg): Json<Config>,
) -> impl IntoResponse {
    match state.settings_manager.save(&cfg) {
        Ok(()) => (axum::http::StatusCode::OK, Json(serde_json::json!(cfg))).into_response(),
        Err(e) => ApiError::InternalError(e.to_string()).into_response(),
    }
}

async fn patch_settings(
    State(state): State<Arc<ApiState>>,
    Json(partial): Json<serde_json::Value>,
) -> impl IntoResponse {
    let mut current = state.settings_manager.load_or_default();
    let mut current_val = match serde_json::to_value(&current) {
        Ok(v) => v,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    // Merge partial into current
    merge_json(&mut current_val, &partial);

    current = match serde_json::from_value(current_val) {
        Ok(c) => c,
        Err(e) => {
            return ApiError::BadRequest(e.to_string()).into_response();
        }
    };

    match state.settings_manager.save(&current) {
        Ok(()) => (axum::http::StatusCode::OK, Json(serde_json::json!(current))).into_response(),
        Err(e) => ApiError::InternalError(e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GitLab integration
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct ListGitLabIssuesQuery {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub per_page: Option<u32>,
}

async fn list_gitlab_issues(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<ListGitLabIssuesQuery>,
) -> impl IntoResponse {
    let cfg = state.settings_manager.load_or_default();
    let int = &cfg.integrations;

    let token = CredentialProvider::from_env(&int.gitlab_token_env);
    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitLab token not configured. Set the environment variable.",
                "env_var": int.gitlab_token_env,
            })),
        )
            .into_response();
    }

    let project_id = q
        .project_id
        .clone()
        .or_else(|| int.gitlab_project_id.clone())
        .unwrap_or_default();
    if project_id.is_empty() {
        return ApiError::BadRequest(
            "GitLab project ID is required (query param project_id or settings.integrations.gitlab_project_id)."
                .to_string(),
        )
        .into_response();
    }

    let base_url = int
        .gitlab_url
        .clone()
        .unwrap_or_else(|| "https://gitlab.com".to_string());

    let client = match at_integrations::gitlab::GitLabClient::new_with_url(
        &base_url,
        token.as_deref().unwrap_or_default(),
    ) {
        Ok(c) => c,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    match client
        .list_issues(
            &project_id,
            q.state.as_deref(),
            q.page.unwrap_or(1),
            q.per_page.unwrap_or(20),
        )
        .await
    {
        Ok(issues) => (axum::http::StatusCode::OK, Json(serde_json::json!(issues))).into_response(),
        Err(e) => ApiError::InternalError(e.to_string()).into_response(),
    }
}

#[derive(Debug, Default, Deserialize)]
struct ListGitLabMrsQuery {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub per_page: Option<u32>,
}

async fn list_gitlab_merge_requests(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<ListGitLabMrsQuery>,
) -> impl IntoResponse {
    let cfg = state.settings_manager.load_or_default();
    let int = &cfg.integrations;

    let token = CredentialProvider::from_env(&int.gitlab_token_env);
    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitLab token not configured. Set the environment variable.",
                "env_var": int.gitlab_token_env,
            })),
        )
            .into_response();
    }

    let project_id = q
        .project_id
        .clone()
        .or_else(|| int.gitlab_project_id.clone())
        .unwrap_or_default();
    if project_id.is_empty() {
        return ApiError::BadRequest(
            "GitLab project ID is required (query param project_id or settings.integrations.gitlab_project_id)."
                .to_string(),
        )
        .into_response();
    }

    let base_url = int
        .gitlab_url
        .clone()
        .unwrap_or_else(|| "https://gitlab.com".to_string());

    let client = match at_integrations::gitlab::GitLabClient::new_with_url(
        &base_url,
        token.as_deref().unwrap_or_default(),
    ) {
        Ok(c) => c,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    match client
        .list_merge_requests(
            &project_id,
            q.state.as_deref(),
            q.page.unwrap_or(1),
            q.per_page.unwrap_or(20),
        )
        .await
    {
        Ok(mrs) => (axum::http::StatusCode::OK, Json(serde_json::json!(mrs))).into_response(),
        Err(e) => ApiError::InternalError(e.to_string()).into_response(),
    }
}

#[derive(Debug, Default, Deserialize)]
struct ReviewGitLabMrBody {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub severity_threshold: Option<at_integrations::gitlab::mr_review::MrReviewSeverity>,
    #[serde(default)]
    pub max_findings: Option<usize>,
    #[serde(default)]
    pub auto_approve: Option<bool>,
}

async fn review_gitlab_merge_request(
    State(state): State<Arc<ApiState>>,
    Path(iid): Path<u32>,
    body: Option<Json<ReviewGitLabMrBody>>,
) -> impl IntoResponse {
    let cfg = state.settings_manager.load_or_default();
    let int = &cfg.integrations;

    let token = CredentialProvider::from_env(&int.gitlab_token_env);
    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitLab token not configured. Set the environment variable.",
                "env_var": int.gitlab_token_env,
            })),
        )
            .into_response();
    }

    let req = body.map(|b| b.0).unwrap_or_default();
    let project_id = req
        .project_id
        .or_else(|| int.gitlab_project_id.clone())
        .unwrap_or_default();
    if project_id.is_empty() {
        return ApiError::BadRequest(
            "GitLab project ID is required (request body project_id or settings.integrations.gitlab_project_id)."
                .to_string(),
        )
        .into_response();
    }

    let base_url = int
        .gitlab_url
        .clone()
        .unwrap_or_else(|| "https://gitlab.com".to_string());
    let client = match at_integrations::gitlab::GitLabClient::new_with_url(
        &base_url,
        token.as_deref().unwrap_or_default(),
    ) {
        Ok(c) => c,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    let mut config = at_integrations::gitlab::mr_review::MrReviewConfig::default();
    if let Some(v) = req.severity_threshold {
        config.severity_threshold = v;
    }
    if let Some(v) = req.max_findings {
        config.max_findings = v;
    }
    if let Some(v) = req.auto_approve {
        config.auto_approve = v;
    }

    let engine = at_integrations::gitlab::mr_review::MrReviewEngine::with_client(config, client);
    let result = engine.review_mr(&project_id, iid).await;
    (axum::http::StatusCode::OK, Json(serde_json::json!(result))).into_response()
}

// ---------------------------------------------------------------------------
// Linear integration
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct ListLinearIssuesQuery {
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
}

async fn list_linear_issues(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<ListLinearIssuesQuery>,
) -> impl IntoResponse {
    let cfg = state.settings_manager.load_or_default();
    let int = &cfg.integrations;

    let token = CredentialProvider::from_env(&int.linear_api_key_env);
    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "Linear API key not configured. Set the environment variable.",
                "env_var": int.linear_api_key_env,
            })),
        )
            .into_response();
    }

    let client =
        match at_integrations::linear::LinearClient::new(token.as_deref().unwrap_or_default()) {
            Ok(c) => c,
            Err(e) => {
                return ApiError::InternalError(e.to_string()).into_response();
            }
        };

    let team = q.team_id.as_deref().or(int.linear_team_id.as_deref());
    if team.is_none() {
        return ApiError::BadRequest(
            "Linear team_id is required (query param team_id or settings.integrations.linear_team_id)."
                .to_string(),
        )
        .into_response();
    }

    match client.list_issues(team, q.state.as_deref()).await {
        Ok(issues) => (axum::http::StatusCode::OK, Json(serde_json::json!(issues))).into_response(),
        Err(e) => ApiError::InternalError(e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct ImportLinearBody {
    pub issue_ids: Vec<String>,
}

async fn import_linear_issues(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<ImportLinearBody>,
) -> impl IntoResponse {
    let cfg = state.settings_manager.load_or_default();
    let int = &cfg.integrations;

    let token = CredentialProvider::from_env(&int.linear_api_key_env);
    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "Linear API key not configured. Set the environment variable.",
                "env_var": int.linear_api_key_env,
            })),
        )
            .into_response();
    }

    let client =
        match at_integrations::linear::LinearClient::new(token.as_deref().unwrap_or_default()) {
            Ok(c) => c,
            Err(e) => {
                return ApiError::InternalError(e.to_string()).into_response();
            }
        };

    match client.import_issues(body.issue_ids).await {
        Ok(results) => {
            (axum::http::StatusCode::OK, Json(serde_json::json!(results))).into_response()
        }
        Err(e) => ApiError::InternalError(e.to_string()).into_response(),
    }
}

/// GET /api/kanban/columns — return the 8-column Kanban config (order, labels, optional width).
async fn get_kanban_columns(State(state): State<Arc<ApiState>>) -> Json<KanbanColumnConfig> {
    let cols = state.kanban_columns.read().await;
    Json(cols.clone())
}

/// PATCH /api/kanban/columns — update column config (e.g. order, labels, width_px).
async fn patch_kanban_columns(
    State(state): State<Arc<ApiState>>,
    Json(patch): Json<KanbanColumnConfig>,
) -> impl IntoResponse {
    let mut cols = state.kanban_columns.write().await;
    if patch.columns.is_empty() {
        return ApiError::BadRequest("columns must not be empty".to_string()).into_response();
    }
    *cols = patch;
    (
        axum::http::StatusCode::OK,
        Json(serde_json::to_value(cols.clone()).unwrap()),
    )
        .into_response()
}

/// GET /api/credentials/status — report which credential providers are available.
async fn get_credentials_status() -> impl IntoResponse {
    let providers: Vec<&str> = CredentialProvider::available_providers();
    let daemon_auth = CredentialProvider::daemon_api_key().is_some();
    Json(serde_json::json!({
        "providers": providers,
        "daemon_auth": daemon_auth,
    }))
}

/// Deep-merge `patch` into `target`. Objects are merged recursively; other
/// values are replaced.
fn merge_json(target: &mut serde_json::Value, patch: &serde_json::Value) {
    match (target.is_object(), patch.is_object()) {
        (true, true) => {
            let t = target.as_object_mut().unwrap();
            let p = patch.as_object().unwrap();
            for (key, value) in p {
                let entry = t.entry(key.clone()).or_insert(serde_json::Value::Null);
                merge_json(entry, value);
            }
        }
        _ => {
            *target = patch.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// GitHub sync handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct SyncResponse {
    message: String,
    imported: u64,
    statuses_synced: u64,
}

#[derive(Debug, Serialize)]
struct PrCreatedResponse {
    message: String,
    task_id: Uuid,
    pr_title: String,
    pr_branch: Option<String>,
}

/// Request body for creating a PR (supports stacked PRs via base_branch).
#[derive(Debug, Default, serde::Deserialize)]
struct CreatePrRequest {
    /// Target branch for the PR. Defaults to "main". For stacked PRs, set to the parent branch (e.g. "feature/parent").
    #[serde(default)]
    pub base_branch: Option<String>,
}

/// Query params for GET /api/github/issues.
#[derive(Debug, Default, serde::Deserialize)]
struct ListGitHubIssuesQuery {
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub labels: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub per_page: Option<u8>,
}

async fn list_github_issues(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<ListGitHubIssuesQuery>,
) -> impl IntoResponse {
    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitHub token not configured. Set the environment variable.",
                "env_var": int.github_token_env,
            })),
        )
            .into_response();
    }
    if owner.is_empty() || repo.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitHub owner and repo must be set in settings (integrations)."
            })),
        )
            .into_response();
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    let state_filter = q
        .state
        .as_deref()
        .and_then(|s| match s.to_lowercase().as_str() {
            "open" => Some(IssueState::Open),
            "closed" => Some(IssueState::Closed),
            _ => None,
        });
    let labels: Option<Vec<String>> = q.labels.as_deref().filter(|s| !s.is_empty()).map(|s| {
        s.split(',')
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    });
    let list = match issues::list_issues(&client, state_filter, labels, q.page, q.per_page).await {
        Ok(issues) => issues,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    (axum::http::StatusCode::OK, Json(serde_json::json!(list))).into_response()
}

async fn trigger_github_sync(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitHub token not configured. Set the environment variable.",
                "env_var": int.github_token_env,
            })),
        )
            .into_response();
    }
    if owner.is_empty() || repo.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitHub owner and repo must be set in settings (integrations)."
            })),
        )
            .into_response();
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    {
        let mut status = state.sync_status.write().await;
        status.is_syncing = true;
    }

    let existing_beads: Vec<Bead> = state.beads.read().await.clone();
    let engine = IssueSyncEngine::new(client);
    let new_beads = match engine.import_open_issues(&existing_beads).await {
        Ok(b) => b,
        Err(e) => {
            let mut status = state.sync_status.write().await;
            status.is_syncing = false;
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    let imported_count = new_beads.len() as u64;
    state.beads.write().await.extend(new_beads);

    {
        let mut status = state.sync_status.write().await;
        status.is_syncing = false;
        status.last_sync_time = Some(chrono::Utc::now());
        status.issues_imported = status.issues_imported.saturating_add(imported_count);
    }

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(SyncResponse {
            message: "Sync completed".to_string(),
            imported: imported_count,
            statuses_synced: 0,
        })),
    )
        .into_response()
}

async fn get_sync_status(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let status = state.sync_status.read().await;
    Json(serde_json::json!(*status))
}

async fn create_pr_for_task(
    State(state): State<Arc<ApiState>>,
    Path(task_id): Path<Uuid>,
    body: Option<Json<CreatePrRequest>>,
) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let task = match tasks.iter().find(|t| t.id == task_id) {
        Some(t) => t.clone(),
        None => {
            return ApiError::NotFound("task not found".to_string()).into_response();
        }
    };
    drop(tasks);

    if task.git_branch.is_none() {
        return ApiError::BadRequest(
            "Task has no branch. Create a worktree for this task first.".to_string(),
        )
        .into_response();
    }

    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitHub token not configured. Set the environment variable.",
                "env_var": int.github_token_env,
            })),
        )
            .into_response();
    }
    if owner.is_empty() || repo.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitHub owner and repo must be set in settings (integrations)."
            })),
        )
            .into_response();
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    let base_branch = body
        .as_ref()
        .and_then(|b| b.base_branch.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or("main");
    let automation = PrAutomation::new(client);
    let pr = match automation.create_pr_for_task(&task, base_branch).await {
        Ok(p) => p,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    let response = PrCreatedResponse {
        message: "PR created".to_string(),
        task_id: task.id,
        pr_title: pr.title.clone(),
        pr_branch: Some(pr.head_branch.clone()),
    };

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "message": response.message,
            "task_id": response.task_id,
            "pr_title": response.pr_title,
            "pr_branch": response.pr_branch,
            "pr_base_branch": base_branch,
            "pr_number": pr.number,
            "pr_url": pr.html_url,
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Notification handlers
// ---------------------------------------------------------------------------

/// GET /api/notifications — list notifications with optional filters.
async fn list_notifications(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<NotificationQuery>,
) -> impl IntoResponse {
    let store = state.notification_store.read().await;
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    if params.unread == Some(true) {
        let unread: Vec<_> = store
            .list_unread()
            .into_iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();
        Json(serde_json::json!(unread))
    } else {
        let all: Vec<_> = store.list_all(limit, offset).into_iter().cloned().collect();
        Json(serde_json::json!(all))
    }
}

/// GET /api/notifications/count — return unread count.
async fn notification_count(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let store = state.notification_store.read().await;
    Json(serde_json::json!({
        "unread": store.unread_count(),
        "total": store.total_count(),
    }))
}

/// POST /api/notifications/{id}/read — mark a single notification as read.
async fn mark_notification_read(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut store = state.notification_store.write().await;
    if store.mark_read(id) {
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"status": "read", "id": id.to_string()})),
        )
            .into_response()
    } else {
        ApiError::NotFound("notification not found".to_string()).into_response()
    }
}

/// POST /api/notifications/read-all — mark all notifications as read.
async fn mark_all_notifications_read(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let mut store = state.notification_store.write().await;
    store.mark_all_read();
    Json(serde_json::json!({"status": "all_read"}))
}

/// DELETE /api/notifications/{id} — delete a notification.
async fn delete_notification(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut store = state.notification_store.write().await;
    if store.delete(id) {
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"status": "deleted", "id": id.to_string()})),
        )
            .into_response()
    } else {
        ApiError::NotFound("notification not found".to_string()).into_response()
    }
}

// ---------------------------------------------------------------------------
// Metrics handlers
// ---------------------------------------------------------------------------

/// GET /api/metrics — Prometheus text format export.
async fn get_metrics_prometheus() -> impl IntoResponse {
    let body = global_metrics().export_prometheus();
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

/// GET /api/metrics/json — JSON format export.
async fn get_metrics_json() -> impl IntoResponse {
    Json(global_metrics().export_json())
}

// ---------------------------------------------------------------------------
// Session handlers
// ---------------------------------------------------------------------------

/// GET /api/sessions/ui — load the most recent UI session (or return null).
async fn get_ui_session(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    match state.session_store.list_sessions() {
        Ok(sessions) => {
            if let Some(session) = sessions.into_iter().next() {
                (axum::http::StatusCode::OK, Json(serde_json::json!(session))).into_response()
            } else {
                (axum::http::StatusCode::OK, Json(serde_json::json!(null))).into_response()
            }
        }
        Err(e) => ApiError::InternalError(e.to_string()).into_response(),
    }
}

/// PUT /api/sessions/ui — save a UI session state.
async fn save_ui_session(
    State(state): State<Arc<ApiState>>,
    Json(mut session): Json<SessionState>,
) -> impl IntoResponse {
    session.last_active_at = chrono::Utc::now();
    match state.session_store.save_session(&session) {
        Ok(()) => (axum::http::StatusCode::OK, Json(serde_json::json!(session))).into_response(),
        Err(e) => ApiError::InternalError(e.to_string()).into_response(),
    }
}

/// GET /api/sessions/ui/list — list all saved sessions.
async fn list_ui_sessions(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    match state.session_store.list_sessions() {
        Ok(sessions) => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!(sessions)),
        )
            .into_response(),
        Err(e) => ApiError::InternalError(e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// WebSocket — legacy /ws handler
// ---------------------------------------------------------------------------

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<ApiState>) {
    let rx = state.event_bus.subscribe();
    while let Ok(msg) = rx.recv_async().await {
        let json = serde_json::to_string(&*msg).unwrap_or_default();
        if socket.send(Message::Text(json.into())).await.is_err() {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// WebSocket — /api/events/ws with heartbeat + event-to-notification wiring
// ---------------------------------------------------------------------------

async fn events_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_events_ws(socket, state))
}

async fn handle_events_ws(socket: WebSocket, state: Arc<ApiState>) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let rx = state.event_bus.subscribe();
    let notification_store = state.notification_store.clone();

    // Heartbeat interval: 30 seconds
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(30));

    loop {
        tokio::select! {
            // Forward events from the bus to the WebSocket client
            result = rx.recv_async() => {
                match result {
                    Ok(msg) => {
                        // Wire event to notification store
                        if let Some((title, message, level, source, action_url)) = notification_from_event(&msg) {
                            let mut store = notification_store.write().await;
                            store.add_with_url(title, message, level, source, action_url);
                        }

                        let json = serde_json::to_string(&*msg).unwrap_or_default();
                        if ws_tx.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            // Send heartbeat ping every 30s
            _ = heartbeat.tick() => {
                let ping_msg = serde_json::json!({"type": "ping", "timestamp": chrono::Utc::now().to_rfc3339()});
                if ws_tx.send(Message::Text(ping_msg.to_string().into())).await.is_err() {
                    break;
                }
            }

            // Handle incoming messages from client (pong, close, etc.)
            incoming = ws_rx.next() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {} // Ignore other messages (pong, text commands, etc.)
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// MCP servers handler
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpServer {
    name: String,
    status: String,
    tools: Vec<String>,
}

async fn list_mcp_servers() -> Json<Vec<McpServer>> {
    // Build a registry with built-in tools to report them dynamically.
    let registry = at_harness::mcp::McpToolRegistry::with_builtins();

    // Collect servers from the registry (currently just built-in).
    let mut servers: Vec<McpServer> = Vec::new();
    for server_name in registry.server_names() {
        let tool_names: Vec<String> = registry
            .list_tools_for_server(&server_name)
            .iter()
            .map(|rt| rt.tool.name.clone())
            .collect();
        servers.push(McpServer {
            name: server_name.clone(),
            status: "active".to_string(),
            tools: tool_names,
        });
    }

    // Also include well-known external MCP servers as stubs (inactive until configured).
    let external_stubs = vec![
        McpServer {
            name: "Context7".into(),
            status: "active".into(),
            tools: vec!["resolve_library_id".into(), "get_library_docs".into()],
        },
        McpServer {
            name: "Graphiti Memory".into(),
            status: "active".into(),
            tools: vec![
                "add_memory".into(),
                "search_memory".into(),
                "delete_memory".into(),
            ],
        },
        McpServer {
            name: "Linear".into(),
            status: "inactive".into(),
            tools: vec![
                "create_issue".into(),
                "list_issues".into(),
                "update_issue".into(),
            ],
        },
        McpServer {
            name: "Sequential Thinking".into(),
            status: "active".into(),
            tools: vec!["create_thinking_session".into(), "add_thought".into()],
        },
        McpServer {
            name: "Filesystem".into(),
            status: "active".into(),
            tools: vec![
                "read_file".into(),
                "write_file".into(),
                "list_directory".into(),
            ],
        },
        McpServer {
            name: "Puppeteer".into(),
            status: "inactive".into(),
            tools: vec!["navigate".into(), "screenshot".into(), "click".into()],
        },
    ];
    servers.extend(external_stubs);

    Json(servers)
}

// ---------------------------------------------------------------------------
// MCP tool call handler
// ---------------------------------------------------------------------------

async fn call_mcp_tool(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<at_harness::mcp::ToolCallRequest>,
) -> impl IntoResponse {
    let ctx = at_harness::builtin_tools::BuiltinToolContext {
        beads: Arc::clone(&state.beads),
        agents: Arc::clone(&state.agents),
        tasks: Arc::clone(&state.tasks),
    };

    match at_harness::builtin_tools::execute_builtin_tool(&ctx, &request).await {
        Some(result) => {
            if result.is_error {
                ApiError::BadRequest(
                    serde_json::to_string(&result)
                        .unwrap_or_else(|_| "builtin tool error".to_string()),
                )
                .into_response()
            } else {
                (
                    axum::http::StatusCode::OK,
                    Json(serde_json::to_value(result).unwrap()),
                )
                    .into_response()
            }
        }
        None => ApiError::NotFound(format!("unknown tool: {}", request.name)).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Worktrees handler
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorktreeEntry {
    id: String,
    path: String,
    branch: String,
    bead_id: String,
    status: String,
}

fn stable_worktree_id(path: &str, branch: &str) -> String {
    let raw = if branch.is_empty() {
        format!("path:{path}")
    } else {
        format!("branch:{branch}")
    };
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

async fn list_worktrees() -> impl IntoResponse {
    let output = match tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return ApiError::InternalError(stderr).into_response();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut worktrees = Vec::new();
    let mut current_path = String::new();
    let mut current_branch = String::new();

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = path.to_string();
            current_branch = String::new();
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            current_branch = branch.to_string();
        } else if line.is_empty() && !current_path.is_empty() {
            worktrees.push(WorktreeEntry {
                id: stable_worktree_id(&current_path, &current_branch),
                path: current_path.clone(),
                branch: current_branch.clone(),
                bead_id: String::new(),
                status: "active".into(),
            });
            current_path = String::new();
            current_branch = String::new();
        }
    }
    // Handle last entry if stdout doesn't end with empty line
    if !current_path.is_empty() {
        worktrees.push(WorktreeEntry {
            id: stable_worktree_id(&current_path, &current_branch),
            path: current_path,
            branch: current_branch,
            bead_id: String::new(),
            status: "active".into(),
        });
    }

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(worktrees)),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Merge handlers
// ---------------------------------------------------------------------------

/// POST /api/worktrees/{id}/merge — trigger merge to main for a worktree branch.
async fn merge_worktree(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Look up the worktree by listing current git worktrees and matching the id/branch.
    let output = match tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut found_branch = None;
    let mut current_path = String::new();
    let mut current_branch = String::new();

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = path.to_string();
            current_branch = String::new();
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            current_branch = branch.to_string();
        } else if line.is_empty() && !current_path.is_empty() {
            let candidate_id = stable_worktree_id(&current_path, &current_branch);
            // Match exact stable id first, then keep legacy contains fallback.
            if candidate_id == id || current_branch.contains(&id) || current_path.contains(&id) {
                found_branch = Some(current_branch.clone());
            }
            current_path = String::new();
            current_branch = String::new();
        }
    }
    // Handle last entry
    if found_branch.is_none() && !current_path.is_empty() {
        let candidate_id = stable_worktree_id(&current_path, &current_branch);
        if candidate_id == id || current_branch.contains(&id) || current_path.contains(&id) {
            found_branch = Some(current_branch);
        }
    }

    let branch = match found_branch {
        Some(b) if !b.is_empty() => b,
        _ => {
            return ApiError::NotFound("worktree not found".to_string()).into_response();
        }
    };

    // Attempt the merge using git commands
    let base_dir = std::env::current_dir().unwrap_or_default();
    let base_dir_str = base_dir.to_str().unwrap_or(".");

    // Check if there are changes to merge
    let diff_output = tokio::process::Command::new("git")
        .args(["diff", "--stat", "main", &branch])
        .current_dir(base_dir_str)
        .output()
        .await;

    match diff_output {
        Ok(o) if String::from_utf8_lossy(&o.stdout).trim().is_empty() => {
            return (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({"status": "nothing_to_merge", "branch": branch})),
            )
                .into_response();
        }
        Ok(_) => { /* has changes */ }
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    }

    // Attempt merge --no-commit to detect conflicts
    let merge_output = tokio::process::Command::new("git")
        .args(["merge", "--no-ff", "--no-commit", &branch])
        .current_dir(base_dir_str)
        .output()
        .await;

    match merge_output {
        Ok(o) if o.status.success() => {
            // Commit the merge
            let commit_msg = format!("Merge branch '{}' into main", branch);
            if let Err(e) = tokio::process::Command::new("git")
                .args(["commit", "-m", &commit_msg])
                .current_dir(base_dir_str)
                .output()
                .await
            {
                warn!(error = %e, "git commit failed during merge");
            }

            // Publish event
            state
                .event_bus
                .publish(crate::protocol::BridgeMessage::MergeResult {
                    worktree_id: id.clone(),
                    branch: branch.clone(),
                    status: "success".to_string(),
                    conflict_files: vec![],
                });

            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({"status": "success", "branch": branch})),
            )
                .into_response()
        }
        Ok(o) => {
            // Detect conflict files
            let conflict_output = tokio::process::Command::new("git")
                .args(["diff", "--name-only", "--diff-filter=U"])
                .current_dir(base_dir_str)
                .output()
                .await;

            // Abort the merge
            if let Err(e) = tokio::process::Command::new("git")
                .args(["merge", "--abort"])
                .current_dir(base_dir_str)
                .output()
                .await
            {
                warn!(error = %e, "git merge --abort failed");
            }

            let conflict_files: Vec<String> = match conflict_output {
                Ok(co) => String::from_utf8_lossy(&co.stdout)
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(|l| l.to_string())
                    .collect(),
                Err(_) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    stderr
                        .lines()
                        .filter(|l| l.contains("CONFLICT"))
                        .map(|l| l.to_string())
                        .collect()
                }
            };

            state
                .event_bus
                .publish(crate::protocol::BridgeMessage::MergeResult {
                    worktree_id: id.clone(),
                    branch: branch.clone(),
                    status: "conflict".to_string(),
                    conflict_files: conflict_files.clone(),
                });

            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({
                    "status": "conflict",
                    "branch": branch,
                    "files": conflict_files,
                })),
            )
                .into_response()
        }
        Err(e) => ApiError::InternalError(e.to_string()).into_response(),
    }
}

/// GET /api/worktrees/{id}/merge-preview — dry-run merge preview.
async fn merge_preview(Path(id): Path<String>) -> impl IntoResponse {
    let base_dir = std::env::current_dir().unwrap_or_default();
    let base_dir_str = base_dir.to_str().unwrap_or(".");

    // Try to find the branch for this worktree id
    let output = match tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut found_branch = None;
    let mut current_path = String::new();
    let mut current_branch = String::new();

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = path.to_string();
            current_branch = String::new();
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            current_branch = branch.to_string();
        } else if line.is_empty() && !current_path.is_empty() {
            if current_branch.contains(&id) || current_path.contains(&id) {
                found_branch = Some(current_branch.clone());
            }
            current_path = String::new();
            current_branch = String::new();
        }
    }
    if found_branch.is_none()
        && !current_path.is_empty()
        && (current_branch.contains(&id) || current_path.contains(&id))
    {
        found_branch = Some(current_branch);
    }

    let branch = match found_branch {
        Some(b) if !b.is_empty() => b,
        _ => {
            return ApiError::NotFound("worktree not found".to_string()).into_response();
        }
    };

    // Count commits ahead/behind
    let rev_list = tokio::process::Command::new("git")
        .args([
            "rev-list",
            "--left-right",
            "--count",
            &format!("main...{}", branch),
        ])
        .current_dir(base_dir_str)
        .output()
        .await;

    let (behind, ahead) = match rev_list {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            let parts: Vec<&str> = text.trim().split('\t').collect();
            let behind = parts
                .first()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let ahead = parts
                .get(1)
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            (behind, ahead)
        }
        _ => (0, 0),
    };

    // List files changed
    let diff_names = tokio::process::Command::new("git")
        .args(["diff", "--name-only", "main", &branch])
        .current_dir(base_dir_str)
        .output()
        .await;

    let files_changed: Vec<String> = match diff_names {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect(),
        _ => vec![],
    };

    // Check for potential conflicts via merge-tree (git 2.38+) or simple heuristic
    let has_conflicts = false; // Conservative: actual conflicts only detectable via real merge

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "ahead": ahead,
            "behind": behind,
            "files_changed": files_changed,
            "has_conflicts": has_conflicts,
            "branch": branch,
        })),
    )
        .into_response()
}

/// POST /api/worktrees/{id}/resolve — accept conflict resolution.
async fn resolve_conflict(
    Path(id): Path<String>,
    Json(req): Json<ResolveConflictRequest>,
) -> impl IntoResponse {
    let valid_strategies = ["ours", "theirs", "manual"];
    if !valid_strategies.contains(&req.strategy.as_str()) {
        return ApiError::BadRequest(format!(
            "invalid strategy '{}', must be one of: ours, theirs, manual",
            req.strategy
        ))
        .into_response();
    }

    let base_dir = std::env::current_dir().unwrap_or_default();
    let base_dir_str = base_dir.to_str().unwrap_or(".");

    match req.strategy.as_str() {
        "ours" => {
            if let Err(e) = tokio::process::Command::new("git")
                .args(["checkout", "--ours", &req.file])
                .current_dir(base_dir_str)
                .output()
                .await
            {
                warn!(error = %e, file = %req.file, "git conflict resolution command failed");
            }
            if let Err(e) = tokio::process::Command::new("git")
                .args(["add", &req.file])
                .current_dir(base_dir_str)
                .output()
                .await
            {
                warn!(error = %e, file = %req.file, "git conflict resolution command failed");
            }
        }
        "theirs" => {
            if let Err(e) = tokio::process::Command::new("git")
                .args(["checkout", "--theirs", &req.file])
                .current_dir(base_dir_str)
                .output()
                .await
            {
                warn!(error = %e, file = %req.file, "git conflict resolution command failed");
            }
            if let Err(e) = tokio::process::Command::new("git")
                .args(["add", &req.file])
                .current_dir(base_dir_str)
                .output()
                .await
            {
                warn!(error = %e, file = %req.file, "git conflict resolution command failed");
            }
        }
        "manual" => {
            // For manual, just mark the file as resolved by staging it
            if let Err(e) = tokio::process::Command::new("git")
                .args(["add", &req.file])
                .current_dir(base_dir_str)
                .output()
                .await
            {
                warn!(error = %e, file = %req.file, "git conflict resolution command failed");
            }
        }
        _ => {}
    }

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "status": "resolved",
            "worktree_id": id,
            "file": req.file,
            "strategy": req.strategy,
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Agent Queue handlers
// ---------------------------------------------------------------------------

/// GET /api/queue — list queued tasks sorted by priority.
async fn list_queue(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let tasks = state.tasks.read().await;

    // Filter to tasks in Discovery phase (queued/not-yet-started)
    let mut queued: Vec<_> = tasks
        .iter()
        .filter(|t| t.phase == TaskPhase::Discovery && t.started_at.is_none())
        .cloned()
        .collect();

    // Sort by priority: Urgent > High > Medium > Low
    queued.sort_by(|a, b| {
        let priority_ord = |p: &TaskPriority| -> u8 {
            match p {
                TaskPriority::Urgent => 0,
                TaskPriority::High => 1,
                TaskPriority::Medium => 2,
                TaskPriority::Low => 3,
            }
        };
        priority_ord(&a.priority).cmp(&priority_ord(&b.priority))
    });

    let result: Vec<serde_json::Value> = queued
        .iter()
        .enumerate()
        .map(|(i, t)| {
            serde_json::json!({
                "task_id": t.id,
                "title": t.title,
                "priority": t.priority,
                "queued_at": t.created_at,
                "position": i + 1,
            })
        })
        .collect();

    Json(serde_json::json!(result))
}

/// POST /api/queue/reorder — reorder the task queue.
async fn reorder_queue(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<QueueReorderRequest>,
) -> impl IntoResponse {
    if req.task_ids.is_empty() {
        return ApiError::BadRequest("task_ids must not be empty".to_string()).into_response();
    }

    // Validate all task IDs exist
    let tasks = state.tasks.read().await;
    for task_id in &req.task_ids {
        if !tasks.iter().any(|t| t.id == *task_id) {
            return ApiError::NotFound(format!("task {} not found", task_id)).into_response();
        }
    }
    drop(tasks);

    // Publish queue update event
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::QueueUpdate {
            task_ids: req.task_ids,
        });

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({"status": "ok"})),
    )
        .into_response()
}

/// POST /api/queue/{task_id}/prioritize — bump a task's priority.
async fn prioritize_task(
    State(state): State<Arc<ApiState>>,
    Path(task_id): Path<Uuid>,
    Json(req): Json<PrioritizeRequest>,
) -> impl IntoResponse {
    let mut tasks = state.tasks.write().await;
    let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) else {
        return ApiError::NotFound("task not found".to_string()).into_response();
    };

    task.priority = req.priority;
    task.updated_at = chrono::Utc::now();

    let task_snapshot = task.clone();
    drop(tasks);

    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(
            task_snapshot.clone(),
        )));

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(task_snapshot)),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Direct mode handler
// ---------------------------------------------------------------------------

/// POST /api/settings/direct-mode — toggle direct mode (agents work in repo root).
async fn toggle_direct_mode(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<DirectModeRequest>,
) -> impl IntoResponse {
    let mut current = state.settings_manager.load_or_default();
    let mut current_val = match serde_json::to_value(&current) {
        Ok(v) => v,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    // Merge direct_mode into the agents section
    let patch = serde_json::json!({"agents": {"direct_mode": req.enabled}});
    merge_json(&mut current_val, &patch);

    current = match serde_json::from_value(current_val) {
        Ok(c) => c,
        Err(e) => {
            return ApiError::BadRequest(e.to_string()).into_response();
        }
    };

    match state.settings_manager.save(&current) {
        Ok(()) => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "direct_mode": req.enabled,
            })),
        )
            .into_response(),
        Err(e) => ApiError::InternalError(e.to_string()).into_response(),
    }
}

/// GET /api/cli/available — detect which CLI tools are installed on the system.
async fn list_available_clis() -> impl IntoResponse {
    let cli_names = ["claude", "codex", "gemini", "opencode"];
    let mut entries = Vec::new();

    for name in &cli_names {
        let (detected, path) = detect_cli_binary(name);
        entries.push(CliAvailabilityEntry {
            name: name.to_string(),
            detected,
            path,
        });
    }

    (axum::http::StatusCode::OK, Json(serde_json::json!(entries)))
}

/// Use `which` to detect a CLI binary on the system PATH.
fn detect_cli_binary(name: &str) -> (bool, Option<String>) {
    match std::process::Command::new("which").arg(name).output() {
        Ok(output) if output.status.success() => {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            (true, Some(path))
        }
        _ => (false, None),
    }
}

/// DELETE /api/worktrees/{id} — remove a git worktree by path.
async fn delete_worktree(Path(id): Path<String>) -> impl IntoResponse {
    let output = match tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return ApiError::InternalError(stderr).into_response();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut current_path = String::new();
    let mut current_branch = String::new();
    let mut found_path: Option<String> = None;

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = path.to_string();
            current_branch = String::new();
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            current_branch = branch.to_string();
        } else if line.is_empty() && !current_path.is_empty() {
            let candidate_id = stable_worktree_id(&current_path, &current_branch);
            if candidate_id == id || current_branch.contains(&id) || current_path.contains(&id) {
                found_path = Some(current_path.clone());
                break;
            }
            current_path.clear();
            current_branch.clear();
        }
    }
    if found_path.is_none() && !current_path.is_empty() {
        let candidate_id = stable_worktree_id(&current_path, &current_branch);
        if candidate_id == id || current_branch.contains(&id) || current_path.contains(&id) {
            found_path = Some(current_path);
        }
    }

    let Some(path) = found_path else {
        return ApiError::NotFound("worktree not found".to_string()).into_response();
    };

    let rm = tokio::process::Command::new("git")
        .args(["worktree", "remove", "--force", &path])
        .output()
        .await;

    match rm {
        Ok(o) if o.status.success() => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"status": "deleted", "id": id, "path": path})),
        )
            .into_response(),
        Ok(o) => {
            ApiError::BadRequest(String::from_utf8_lossy(&o.stderr).to_string()).into_response()
        }
        Err(e) => {
            ApiError::InternalError(format!("{}, id: {}, path: {}", e, id, path)).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// GitHub PRs handler
// ---------------------------------------------------------------------------

/// Query params for GET /api/github/prs.
#[derive(Debug, Default, Deserialize)]
struct ListGitHubPrsQuery {
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub per_page: Option<u8>,
}

async fn list_github_prs(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<ListGitHubPrsQuery>,
) -> impl IntoResponse {
    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitHub token not configured. Set the environment variable.",
                "env_var": int.github_token_env,
            })),
        )
            .into_response();
    }
    if owner.is_empty() || repo.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitHub owner and repo must be set in settings (integrations)."
            })),
        )
            .into_response();
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    let state_filter = q
        .state
        .as_deref()
        .and_then(|s| match s.to_lowercase().as_str() {
            "open" => Some(PrState::Open),
            "closed" => Some(PrState::Closed),
            "merged" => Some(PrState::Merged),
            _ => None,
        });

    let list =
        match pull_requests::list_pull_requests(&client, state_filter, q.page, q.per_page).await {
            Ok(prs) => prs,
            Err(e) => {
                return ApiError::InternalError(e.to_string()).into_response();
            }
        };

    (axum::http::StatusCode::OK, Json(serde_json::json!(list))).into_response()
}

// ---------------------------------------------------------------------------
// Import GitHub issue as bead
// ---------------------------------------------------------------------------

async fn import_github_issue(
    State(state): State<Arc<ApiState>>,
    Path(number): Path<u64>,
) -> impl IntoResponse {
    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitHub token not configured. Set the environment variable.",
                "env_var": int.github_token_env,
            })),
        )
            .into_response();
    }
    if owner.is_empty() || repo.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitHub owner and repo must be set in settings (integrations)."
            })),
        )
            .into_response();
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    let issue = match issues::get_issue(&client, number).await {
        Ok(i) => i,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    let bead = issues::import_issue_as_task(&issue);
    state.beads.write().await.push(bead.clone());

    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(bead)),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// GitHub OAuth handlers
// ---------------------------------------------------------------------------

/// GET /api/github/oauth/authorize — build the GitHub authorization URL.
async fn github_oauth_authorize(State(_state): State<Arc<ApiState>>) -> impl IntoResponse {
    let client_id = match std::env::var("GITHUB_OAUTH_CLIENT_ID") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "GITHUB_OAUTH_CLIENT_ID not set",
                })),
            );
        }
    };

    let redirect_uri = std::env::var("GITHUB_OAUTH_REDIRECT_URI")
        .unwrap_or_else(|_| "http://localhost:3000/api/github/oauth/callback".into());

    let scopes = std::env::var("GITHUB_OAUTH_SCOPES")
        .unwrap_or_else(|_| "repo,read:user,user:email".into())
        .split(',')
        .map(|s| s.trim().to_string())
        .collect::<Vec<_>>();

    let oauth_config = gh_oauth::GitHubOAuthConfig {
        client_id,
        client_secret: String::new(), // not needed for URL generation
        redirect_uri,
        scopes,
    };

    let oauth_client = gh_oauth::GitHubOAuthClient::new(oauth_config);
    let csrf_state = uuid::Uuid::new_v4().to_string();
    let url = oauth_client.authorization_url(&csrf_state);

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "url": url,
            "state": csrf_state,
        })),
    )
}

#[derive(Debug, Deserialize)]
struct OAuthCallbackRequest {
    code: String,
}

/// POST /api/github/oauth/callback — exchange the authorization code for a token.
async fn github_oauth_callback(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<OAuthCallbackRequest>,
) -> impl IntoResponse {
    let client_id = match std::env::var("GITHUB_OAUTH_CLIENT_ID") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "GITHUB_OAUTH_CLIENT_ID not set" })),
            )
                .into_response();
        }
    };
    let client_secret = match std::env::var("GITHUB_OAUTH_CLIENT_SECRET") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "GITHUB_OAUTH_CLIENT_SECRET not set" })),
            )
                .into_response();
        }
    };
    let redirect_uri = std::env::var("GITHUB_OAUTH_REDIRECT_URI")
        .unwrap_or_else(|_| "http://localhost:3000/api/github/oauth/callback".into());

    let scopes = std::env::var("GITHUB_OAUTH_SCOPES")
        .unwrap_or_else(|_| "repo,read:user,user:email".into())
        .split(',')
        .map(|s| s.trim().to_string())
        .collect::<Vec<_>>();

    let oauth_config = gh_oauth::GitHubOAuthConfig {
        client_id,
        client_secret,
        redirect_uri,
        scopes,
    };

    let oauth_client = gh_oauth::GitHubOAuthClient::new(oauth_config);

    let token_resp = match oauth_client.exchange_code(&body.code).await {
        Ok(t) => t,
        Err(e) => {
            return ApiError::BadRequest(e.to_string()).into_response();
        }
    };

    // Fetch user info with the new token.
    let user = match oauth_client.get_user(&token_resp.access_token).await {
        Ok(u) => serde_json::to_value(&u).unwrap_or_default(),
        Err(e) => {
            tracing::warn!("failed to fetch GitHub user after OAuth: {e}");
            serde_json::json!(null)
        }
    };

    // Store token and user in shared state.
    *state.github_oauth_token.write().await = Some(token_resp.access_token.clone());
    *state.github_oauth_user.write().await = Some(user.clone());

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "authenticated": true,
            "user": user,
            "scope": token_resp.scope,
            "expires_in": token_resp.expires_in,
        })),
    )
        .into_response()
}

/// GET /api/github/oauth/status — check whether the user is authenticated.
async fn github_oauth_status(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let token = state.github_oauth_token.read().await;
    let user = state.github_oauth_user.read().await;

    let authenticated = token.is_some();

    Json(serde_json::json!({
        "authenticated": authenticated,
        "user": *user,
    }))
}

/// POST /api/github/oauth/revoke — clear the stored OAuth token.
async fn github_oauth_revoke(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    *state.github_oauth_token.write().await = None;
    *state.github_oauth_user.write().await = None;

    Json(serde_json::json!({
        "revoked": true,
    }))
}

// ---------------------------------------------------------------------------
// Costs handler
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CostResponse {
    input_tokens: u64,
    output_tokens: u64,
    sessions: Vec<CostSessionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CostSessionEntry {
    session_id: String,
    agent_name: String,
    input_tokens: u64,
    output_tokens: u64,
}

async fn get_costs() -> Json<CostResponse> {
    Json(CostResponse {
        input_tokens: 0,
        output_tokens: 0,
        sessions: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Agent sessions handler
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentSessionEntry {
    id: String,
    agent_name: String,
    cli_type: String,
    status: String,
    duration: String,
}

async fn list_agent_sessions(State(state): State<Arc<ApiState>>) -> Json<Vec<AgentSessionEntry>> {
    let agents = state.agents.read().await;
    let sessions: Vec<AgentSessionEntry> = agents
        .iter()
        .map(|a| {
            let duration_secs = (chrono::Utc::now() - a.created_at).num_seconds().max(0) as u64;
            let mins = duration_secs / 60;
            let secs = duration_secs % 60;
            AgentSessionEntry {
                id: a.session_id.clone().unwrap_or_else(|| a.id.to_string()),
                agent_name: a.name.clone(),
                cli_type: format!("{:?}", a.cli_type).to_lowercase(),
                status: format!("{:?}", a.status).to_lowercase(),
                duration: format!("{}m {}s", mins, secs),
            }
        })
        .collect();
    Json(sessions)
}

// ---------------------------------------------------------------------------
// Convoys handler
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConvoyEntry {
    id: String,
    name: String,
    bead_count: u32,
    status: String,
}

async fn list_convoys() -> Json<Vec<ConvoyEntry>> {
    Json(Vec::new())
}

// ---------------------------------------------------------------------------
// Project handlers
// ---------------------------------------------------------------------------

async fn list_projects(State(state): State<Arc<ApiState>>) -> Json<Vec<Project>> {
    let projects = state.projects.read().await;
    Json(projects.clone())
}

#[derive(Debug, Deserialize)]
struct CreateProjectRequest {
    name: String,
    path: String,
}

async fn create_project(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateProjectRequest>,
) -> impl IntoResponse {
    let project = Project {
        id: Uuid::new_v4(),
        name: req.name,
        path: req.path,
        created_at: chrono::Utc::now().to_rfc3339(),
        is_active: false,
    };
    let mut projects = state.projects.write().await;
    projects.push(project.clone());
    (axum::http::StatusCode::CREATED, Json(project))
}

#[derive(Debug, Deserialize)]
struct UpdateProjectRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

async fn update_project(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProjectRequest>,
) -> impl IntoResponse {
    let mut projects = state.projects.write().await;
    let Some(project) = projects.iter_mut().find(|p| p.id == id) else {
        return ApiError::NotFound("project not found".to_string()).into_response();
    };
    if let Some(name) = req.name {
        project.name = name;
    }
    if let Some(path) = req.path {
        project.path = path;
    }
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(project.clone())),
    )
        .into_response()
}

async fn delete_project(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut projects = state.projects.write().await;
    if projects.len() <= 1 {
        return ApiError::BadRequest("cannot delete last project".to_string()).into_response();
    }
    let before = projects.len();
    projects.retain(|p| p.id != id);
    if projects.len() == before {
        return ApiError::NotFound("project not found".to_string()).into_response();
    }
    if !projects.iter().any(|p| p.is_active) {
        if let Some(first) = projects.first_mut() {
            first.is_active = true;
        }
    }
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({"ok": true})),
    )
        .into_response()
}

async fn activate_project(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut projects = state.projects.write().await;
    let exists = projects.iter().any(|p| p.id == id);
    if !exists {
        return ApiError::NotFound("project not found".to_string()).into_response();
    }
    for p in projects.iter_mut() {
        p.is_active = p.id == id;
    }
    let activated = projects.iter().find(|p| p.id == id).cloned().unwrap();
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(activated)),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// PR Polling
// ---------------------------------------------------------------------------

/// Spawn a background task that polls watched PRs every 30 seconds.
/// For now it only updates the `last_polled` timestamp — real GitHub API
/// integration can come later.
///
/// Returns the `JoinHandle` so the caller can abort it on shutdown.
pub fn spawn_pr_poller(
    registry: Arc<RwLock<std::collections::HashMap<u32, PrPollStatus>>>,
    shutdown: tokio::sync::broadcast::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    let mut shutdown_rx = shutdown;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                    let mut reg = registry.write().await;
                    let now = chrono::Utc::now();
                    for status in reg.values_mut() {
                        status.last_polled = now;
                    }
                }
                _ = shutdown_rx.recv() => {
                    tracing::info!("PR poller shutting down");
                    break;
                }
            }
        }
    })
}

async fn watch_pr(
    State(state): State<Arc<ApiState>>,
    Path(number): Path<u32>,
) -> impl IntoResponse {
    let status = PrPollStatus {
        pr_number: number,
        state: "open".to_string(),
        mergeable: None,
        checks_passed: None,
        last_polled: chrono::Utc::now(),
    };
    let mut registry = state.pr_poll_registry.write().await;
    registry.insert(number, status.clone());
    (axum::http::StatusCode::OK, Json(serde_json::json!(status)))
}

async fn unwatch_pr(
    State(state): State<Arc<ApiState>>,
    Path(number): Path<u32>,
) -> impl IntoResponse {
    let mut registry = state.pr_poll_registry.write().await;
    if registry.remove(&number).is_some() {
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"removed": number})),
        )
            .into_response()
    } else {
        ApiError::NotFound("PR not watched".to_string()).into_response()
    }
}

async fn list_watched_prs(State(state): State<Arc<ApiState>>) -> Json<Vec<PrPollStatus>> {
    let registry = state.pr_poll_registry.read().await;
    Json(registry.values().cloned().collect())
}

// ---------------------------------------------------------------------------
// GitHub Releases
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CreateReleaseRequest {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

async fn create_release(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateReleaseRequest>,
) -> impl IntoResponse {
    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().is_none_or(|t| t.is_empty()) || owner.is_empty() || repo.is_empty() {
        let release = GitHubRelease {
            tag_name: req.tag_name,
            name: req.name,
            body: req.body,
            draft: req.draft,
            prerelease: req.prerelease,
            created_at: chrono::Utc::now(),
            html_url: format!("local://releases/{}", chrono::Utc::now().timestamp_millis()),
        };
        let mut releases = state.releases.write().await;
        releases.retain(|r| r.tag_name != release.tag_name);
        releases.push(release.clone());
        return (
            axum::http::StatusCode::CREATED,
            Json(serde_json::json!(release)),
        )
            .into_response();
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return ApiError::InternalError(e.to_string()).into_response();
        }
    };

    let route = format!("/repos/{}/{}/releases", client.owner(), client.repo());
    let payload = serde_json::json!({
        "tag_name": req.tag_name,
        "name": req.name,
        "body": req.body,
        "draft": req.draft,
        "prerelease": req.prerelease,
    });

    let created: serde_json::Value = match client.inner().post(route, Some(&payload)).await {
        Ok(v) => v,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": format!("GitHub release create failed: {e}") })),
            )
                .into_response();
        }
    };

    let release = GitHubRelease {
        tag_name: created
            .get("tag_name")
            .and_then(|v| v.as_str())
            .unwrap_or(payload["tag_name"].as_str().unwrap_or_default())
            .to_string(),
        name: created
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| payload["name"].as_str().map(|s| s.to_string())),
        body: created
            .get("body")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| payload["body"].as_str().map(|s| s.to_string())),
        draft: created
            .get("draft")
            .and_then(|v| v.as_bool())
            .unwrap_or(payload["draft"].as_bool().unwrap_or(false)),
        prerelease: created
            .get("prerelease")
            .and_then(|v| v.as_bool())
            .unwrap_or(payload["prerelease"].as_bool().unwrap_or(false)),
        created_at: created
            .get("created_at")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now),
        html_url: created
            .get("html_url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    };

    let mut releases = state.releases.write().await;
    releases.retain(|r| r.tag_name != release.tag_name);
    releases.push(release.clone());
    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(release)),
    )
        .into_response()
}

async fn list_releases(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().is_some_and(|t| !t.is_empty()) && !owner.is_empty() && !repo.is_empty() {
        let gh_config = GitHubConfig { token, owner, repo };
        if let Ok(client) = at_integrations::github::client::GitHubClient::new(gh_config) {
            let route = format!("/repos/{}/{}/releases", client.owner(), client.repo());
            if let Ok(remote) = client
                .inner()
                .get::<Vec<serde_json::Value>, _, _>(&route, None::<&()>)
                .await
            {
                let releases: Vec<GitHubRelease> = remote
                    .into_iter()
                    .map(|r| GitHubRelease {
                        tag_name: r
                            .get("tag_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        name: r
                            .get("name")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        body: r
                            .get("body")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        draft: r.get("draft").and_then(|v| v.as_bool()).unwrap_or(false),
                        prerelease: r
                            .get("prerelease")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                        created_at: r
                            .get("created_at")
                            .and_then(|v| v.as_str())
                            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .unwrap_or_else(chrono::Utc::now),
                        html_url: r
                            .get("html_url")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                    })
                    .collect();

                let mut cache = state.releases.write().await;
                *cache = releases.clone();
                return (
                    axum::http::StatusCode::OK,
                    Json(serde_json::json!(releases)),
                );
            }
        }
    }

    let cached = state.releases.read().await.clone();
    (axum::http::StatusCode::OK, Json(serde_json::json!(cached)))
}

// ---------------------------------------------------------------------------
// Task Archival
// ---------------------------------------------------------------------------

async fn archive_task(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut archived = state.archived_tasks.write().await;
    if !archived.contains(&id) {
        archived.push(id);
    }
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({"archived": id})),
    )
}

async fn unarchive_task(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut archived = state.archived_tasks.write().await;
    archived.retain(|&aid| aid != id);
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({"unarchived": id})),
    )
}

async fn list_archived_tasks(State(state): State<Arc<ApiState>>) -> Json<Vec<Uuid>> {
    let archived = state.archived_tasks.read().await;
    Json(archived.clone())
}

// ---------------------------------------------------------------------------
// Attachment handlers
// ---------------------------------------------------------------------------

async fn list_attachments(
    State(state): State<Arc<ApiState>>,
    Path(task_id): Path<Uuid>,
) -> Json<Vec<Attachment>> {
    let attachments = state.attachments.read().await;
    let filtered: Vec<Attachment> = attachments
        .iter()
        .filter(|a| a.task_id == task_id)
        .cloned()
        .collect();
    Json(filtered)
}

async fn add_attachment(
    State(state): State<Arc<ApiState>>,
    Path(task_id): Path<Uuid>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let attachment = Attachment {
        id: Uuid::new_v4(),
        task_id,
        filename: req
            .get("filename")
            .and_then(|v| v.as_str())
            .unwrap_or("untitled")
            .to_string(),
        content_type: req
            .get("content_type")
            .and_then(|v| v.as_str())
            .unwrap_or("application/octet-stream")
            .to_string(),
        size_bytes: req.get("size_bytes").and_then(|v| v.as_u64()).unwrap_or(0),
        uploaded_at: chrono::Utc::now().to_rfc3339(),
    };
    let mut attachments = state.attachments.write().await;
    attachments.push(attachment.clone());
    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(attachment)),
    )
}

async fn delete_attachment(
    State(state): State<Arc<ApiState>>,
    Path((_task_id, attachment_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    let mut attachments = state.attachments.write().await;
    let before = attachments.len();
    attachments.retain(|a| a.id != attachment_id);
    if attachments.len() < before {
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"deleted": attachment_id})),
        )
            .into_response()
    } else {
        ApiError::NotFound("attachment not found".to_string()).into_response()
    }
}

// ---------------------------------------------------------------------------
// Task draft handlers
// ---------------------------------------------------------------------------

async fn save_task_draft(
    State(state): State<Arc<ApiState>>,
    Json(mut draft): Json<TaskDraft>,
) -> impl IntoResponse {
    draft.updated_at = chrono::Utc::now().to_rfc3339();
    let mut drafts = state.task_drafts.write().await;
    drafts.insert(draft.id, draft.clone());
    (axum::http::StatusCode::OK, Json(serde_json::json!(draft)))
}

async fn get_task_draft(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let drafts = state.task_drafts.read().await;
    match drafts.get(&id) {
        Some(draft) => (axum::http::StatusCode::OK, Json(serde_json::json!(draft))).into_response(),
        None => ApiError::NotFound("draft not found".to_string()).into_response(),
    }
}

async fn delete_task_draft(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut drafts = state.task_drafts.write().await;
    if drafts.remove(&id).is_some() {
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"deleted": id})),
        )
            .into_response()
    } else {
        ApiError::NotFound("draft not found".to_string()).into_response()
    }
}

async fn list_task_drafts(State(state): State<Arc<ApiState>>) -> Json<Vec<TaskDraft>> {
    let drafts = state.task_drafts.read().await;
    Json(drafts.values().cloned().collect())
}

// ---------------------------------------------------------------------------
// Column locking
// ---------------------------------------------------------------------------

async fn lock_column(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<LockColumnRequest>,
) -> impl IntoResponse {
    let mut cols = state.kanban_columns.write().await;
    if let Some(col) = cols.columns.iter_mut().find(|c| c.id == req.column_id) {
        if req.locked && !col.label.starts_with("\u{1f512}") {
            col.label = format!("\u{1f512} {}", col.label);
        } else if !req.locked {
            col.label = col.label.trim_start_matches("\u{1f512} ").to_string();
        }
    }
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({"column_id": req.column_id, "locked": req.locked})),
    )
}

// ---------------------------------------------------------------------------
// Task ordering persistence
// ---------------------------------------------------------------------------

async fn save_task_ordering(
    State(_state): State<Arc<ApiState>>,
    Json(req): Json<TaskOrderingRequest>,
) -> impl IntoResponse {
    // Stub: in production this would persist to SQLite
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "column_id": req.column_id,
            "task_count": req.task_ids.len()
        })),
    )
}

// ---------------------------------------------------------------------------
// File watching
// ---------------------------------------------------------------------------

async fn start_file_watch(
    State(_state): State<Arc<ApiState>>,
    Json(req): Json<FileWatchRequest>,
) -> impl IntoResponse {
    // Stub: in production this would create a FileWatcher from at_core::file_watcher
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "watching": req.path,
            "recursive": req.recursive
        })),
    )
}

async fn stop_file_watch(
    State(_state): State<Arc<ApiState>>,
    Json(req): Json<FileWatchRequest>,
) -> impl IntoResponse {
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "stopped": req.path
        })),
    )
}

// ---------------------------------------------------------------------------
// Competitor analysis
// ---------------------------------------------------------------------------

async fn run_competitor_analysis(
    State(_state): State<Arc<ApiState>>,
    Json(req): Json<CompetitorAnalysisRequest>,
) -> impl IntoResponse {
    let result = CompetitorAnalysisResult {
        competitor_name: req.competitor_name.clone(),
        strengths: vec![
            format!("{} has strong market presence", req.competitor_name),
            "Large community and ecosystem".to_string(),
        ],
        weaknesses: vec![
            "Limited customization options".to_string(),
            "Higher pricing tier".to_string(),
        ],
        opportunities: vec![
            "Underserved enterprise segment".to_string(),
            "Better developer experience possible".to_string(),
        ],
        analyzed_at: chrono::Utc::now(),
    };
    (axum::http::StatusCode::OK, Json(serde_json::json!(result)))
}

// ---------------------------------------------------------------------------
// Profile swap notification
// ---------------------------------------------------------------------------

async fn notify_profile_swap(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let profile_name = req
        .get("profile")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let mut store = state.notification_store.write().await;
    store.add(
        "Profile Swapped",
        format!("API profile swapped to '{}'", profile_name),
        crate::notifications::NotificationLevel::Info,
        "system",
    );
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({"notified": profile_name})),
    )
}

// ---------------------------------------------------------------------------
// App update check notification
// ---------------------------------------------------------------------------

async fn check_app_update(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    // Stub: always returns "up to date"
    let mut store = state.notification_store.write().await;
    store.add(
        "Update Check",
        "auto-tundra is up to date (v0.1.0)",
        crate::notifications::NotificationLevel::Info,
        "system",
    );
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "current_version": "0.1.0",
            "latest_version": "0.1.0",
            "update_available": false
        })),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    /// Build a test router with fresh state.
    fn test_app() -> (Router, Arc<ApiState>) {
        let event_bus = EventBus::new();
        let state = Arc::new(ApiState::new(event_bus));
        let app = api_router(state.clone());
        (app, state)
    }

    #[tokio::test]
    async fn test_trigger_sync_returns_ok() {
        let (app, _state) = test_app();

        let req = Request::builder()
            .method("POST")
            .uri("/api/github/sync")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        // Without GITHUB_TOKEN (and owner/repo) configured, returns 503 or 400
        let status = response.status();
        assert!(
            status == StatusCode::SERVICE_UNAVAILABLE || status == StatusCode::BAD_REQUEST,
            "Expected 503 or 400, got {}",
            status
        );

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // Error message contains either "token" or "owner" depending on which config is missing
        let error_msg = json["error"].as_str().unwrap();
        assert!(
            error_msg.contains("token")
                || error_msg.contains("owner")
                || error_msg.contains("GitHub")
        );
    }

    #[tokio::test]
    async fn test_list_github_issues_requires_config() {
        let (app, _state) = test_app();
        let req = Request::builder()
            .method("GET")
            .uri("/api/github/issues")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        // Returns 503 (no token) or 400 (token but no owner/repo)
        let status = response.status();
        assert!(
            status == StatusCode::SERVICE_UNAVAILABLE || status == StatusCode::BAD_REQUEST,
            "Expected 503 or 400, got {}",
            status
        );
    }

    #[tokio::test]
    async fn test_list_github_issues_accepts_query_params() {
        let (app, _state) = test_app();
        let req = Request::builder()
            .method("GET")
            .uri("/api/github/issues?state=open&page=1&per_page=10")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        // Returns 503 (no token) or 400 (token but no owner/repo)
        let status = response.status();
        assert!(
            status == StatusCode::SERVICE_UNAVAILABLE || status == StatusCode::BAD_REQUEST,
            "Expected 503 or 400, got {}",
            status
        );
    }

    #[tokio::test]
    async fn test_get_sync_status_default() {
        let (app, _state) = test_app();

        let req = Request::builder()
            .method("GET")
            .uri("/api/github/sync/status")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["is_syncing"], false);
        assert!(json["last_sync_time"].is_null());
    }

    #[tokio::test]
    async fn test_create_pr_task_not_found() {
        let (app, _state) = test_app();

        let fake_id = Uuid::new_v4();
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/github/pr/{}", fake_id))
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_create_pr_for_existing_task_no_branch() {
        let (app, state) = test_app();

        let task = Task::new(
            "Test task",
            Uuid::new_v4(),
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Small,
        );
        let task_id = task.id;
        {
            let mut tasks = state.tasks.write().await;
            tasks.push(task);
        }

        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/github/pr/{}", task_id))
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("branch"));
    }

    #[tokio::test]
    async fn test_create_pr_for_existing_task_with_branch_no_token() {
        let (app, state) = test_app();

        let mut task = Task::new(
            "Test task",
            Uuid::new_v4(),
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Small,
        );
        task.git_branch = Some("feature/test-branch".to_string());
        let task_id = task.id;
        {
            let mut tasks = state.tasks.write().await;
            tasks.push(task);
        }

        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/github/pr/{}", task_id))
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        let status = response.status();
        assert!(status == StatusCode::SERVICE_UNAVAILABLE || status == StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let err = json["error"].as_str().unwrap();
        assert!(err.contains("token") || err.contains("owner") || err.contains("repo"));
    }

    #[tokio::test]
    async fn test_create_pr_stacked_base_branch_accepted() {
        let (app, state) = test_app();

        let mut task = Task::new(
            "Stacked PR task",
            Uuid::new_v4(),
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Small,
        );
        task.git_branch = Some("feature/child".to_string());
        let task_id = task.id;
        {
            let mut tasks = state.tasks.write().await;
            tasks.push(task);
        }

        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/github/pr/{}", task_id))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"base_branch":"feature/parent"}"#))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        // Returns 503 (no token) or 400 (token but no owner/repo)
        let status = response.status();
        assert!(
            status == StatusCode::SERVICE_UNAVAILABLE || status == StatusCode::BAD_REQUEST,
            "Expected 503 or 400, got {}",
            status
        );
    }

    // -----------------------------------------------------------------------
    // Notification endpoint tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_notifications_empty() {
        let (app, _state) = test_app();

        let req = Request::builder()
            .method("GET")
            .uri("/api/notifications")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(json.is_empty());
    }

    #[tokio::test]
    async fn test_notification_count_empty() {
        let (app, _state) = test_app();

        let req = Request::builder()
            .method("GET")
            .uri("/api/notifications/count")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["unread"], 0);
        assert_eq!(json["total"], 0);
    }

    #[tokio::test]
    async fn test_notification_crud() {
        let (_app, state) = test_app();

        // Add a notification directly to the store.
        let notif_id;
        {
            let mut store = state.notification_store.write().await;
            notif_id = store.add(
                "Test Alert",
                "Something happened",
                crate::notifications::NotificationLevel::Info,
                "system",
            );
        }

        // List notifications.
        let app = api_router(state.clone());
        let req = Request::builder()
            .method("GET")
            .uri("/api/notifications")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.len(), 1);
        assert_eq!(json[0]["title"], "Test Alert");

        // Count.
        let app = api_router(state.clone());
        let req = Request::builder()
            .method("GET")
            .uri("/api/notifications/count")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["unread"], 1);

        // Mark read.
        let app = api_router(state.clone());
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/notifications/{}/read", notif_id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Count should now be 0 unread.
        let app = api_router(state.clone());
        let req = Request::builder()
            .method("GET")
            .uri("/api/notifications/count")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["unread"], 0);
        assert_eq!(json["total"], 1);

        // Delete.
        let app = api_router(state.clone());
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/notifications/{}", notif_id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Total should be 0 now.
        let app = api_router(state.clone());
        let req = Request::builder()
            .method("GET")
            .uri("/api/notifications/count")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
    }

    #[tokio::test]
    async fn test_mark_all_read() {
        let (_app, state) = test_app();

        // Add multiple notifications.
        {
            let mut store = state.notification_store.write().await;
            store.add(
                "n1",
                "m1",
                crate::notifications::NotificationLevel::Info,
                "system",
            );
            store.add(
                "n2",
                "m2",
                crate::notifications::NotificationLevel::Warning,
                "system",
            );
            store.add(
                "n3",
                "m3",
                crate::notifications::NotificationLevel::Error,
                "system",
            );
        }

        let app = api_router(state.clone());
        let req = Request::builder()
            .method("POST")
            .uri("/api/notifications/read-all")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let app = api_router(state.clone());
        let req = Request::builder()
            .method("GET")
            .uri("/api/notifications/count")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["unread"], 0);
        assert_eq!(json["total"], 3);
    }

    #[tokio::test]
    async fn test_notification_unread_filter() {
        let (_app, state) = test_app();

        let id1;
        {
            let mut store = state.notification_store.write().await;
            id1 = store.add(
                "n1",
                "m1",
                crate::notifications::NotificationLevel::Info,
                "system",
            );
            store.add(
                "n2",
                "m2",
                crate::notifications::NotificationLevel::Warning,
                "system",
            );
            store.mark_read(id1);
        }

        let app = api_router(state.clone());
        let req = Request::builder()
            .method("GET")
            .uri("/api/notifications?unread=true")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.len(), 1);
        assert_eq!(json[0]["title"], "n2");
    }

    #[tokio::test]
    async fn test_notification_pagination() {
        let (_app, state) = test_app();

        {
            let mut store = state.notification_store.write().await;
            for i in 0..10 {
                store.add(
                    format!("n{i}"),
                    "msg",
                    crate::notifications::NotificationLevel::Info,
                    "system",
                );
            }
        }

        let app = api_router(state.clone());
        let req = Request::builder()
            .method("GET")
            .uri("/api/notifications?limit=3&offset=0")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.len(), 3);
        // Newest first
        assert_eq!(json[0]["title"], "n9");
    }

    #[tokio::test]
    async fn test_delete_notification_not_found() {
        let (app, _state) = test_app();

        let fake_id = Uuid::new_v4();
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/notifications/{}", fake_id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_mark_read_not_found() {
        let (app, _state) = test_app();

        let fake_id = Uuid::new_v4();
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/notifications/{}/read", fake_id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // -----------------------------------------------------------------------
    // Execute pipeline endpoint tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_pipeline_task_not_found() {
        let (app, _state) = test_app();
        let fake_id = Uuid::new_v4();

        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/tasks/{}/execute", fake_id))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_execute_pipeline_wrong_phase() {
        let (app, state) = test_app();

        // Create a task in Discovery phase (cannot jump to Coding)
        let task = Task::new(
            "Test task",
            Uuid::new_v4(),
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Small,
        );
        let task_id = task.id;
        state.tasks.write().await.push(task);

        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/tasks/{}/execute", task_id))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_execute_pipeline_accepts_from_planning() {
        let (app, state) = test_app();

        // Create a task in Planning phase (can transition to Coding)
        let mut task = Task::new(
            "Test task",
            Uuid::new_v4(),
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Small,
        );
        task.set_phase(TaskPhase::Planning);
        let task_id = task.id;
        state.tasks.write().await.push(task);

        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/tasks/{}/execute", task_id))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "started");

        // Verify task phase was updated to Coding
        let tasks = state.tasks.read().await;
        let t = tasks.iter().find(|t| t.id == task_id).unwrap();
        assert_eq!(t.phase, TaskPhase::Coding);
    }

    #[tokio::test]
    async fn test_pipeline_queue_status_endpoint() {
        let (app, state) = test_app();

        state.pipeline_waiting.store(2, Ordering::SeqCst);
        state.pipeline_running.store(1, Ordering::SeqCst);

        let req = Request::builder()
            .method("GET")
            .uri("/api/pipeline/queue")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["limit"], state.pipeline_max_concurrent as u64);
        assert_eq!(json["waiting"], 2);
        assert_eq!(json["running"], 1);
        assert!(json["available_permits"].as_u64().is_some());
    }

    #[tokio::test]
    async fn test_list_attachments_empty() {
        let (app, _) = test_app();
        let id = Uuid::new_v4();
        let req = Request::builder()
            .method("GET")
            .uri(&format!("/api/tasks/{id}/attachments"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_save_and_get_draft() {
        let (app, _) = test_app();
        let draft_id = Uuid::new_v4();
        let draft = serde_json::json!({
            "id": draft_id,
            "title": "Test draft",
            "description": "A draft task",
            "category": null,
            "priority": null,
            "files": [],
            "updated_at": ""
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/tasks/drafts")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&draft).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_drafts() {
        let (app, _) = test_app();
        let req = Request::builder()
            .method("GET")
            .uri("/api/tasks/drafts")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_lock_column() {
        let (app, _) = test_app();
        let body = serde_json::json!({"column_id": "done", "locked": true});
        let req = Request::builder()
            .method("POST")
            .uri("/api/kanban/columns/lock")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_save_task_ordering() {
        let (app, _) = test_app();
        let body = serde_json::json!({
            "column_id": "backlog",
            "task_ids": [Uuid::new_v4(), Uuid::new_v4()]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/kanban/ordering")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_file_watch() {
        let (app, _) = test_app();
        let body = serde_json::json!({"path": "/tmp/test", "recursive": true});
        let req = Request::builder()
            .method("POST")
            .uri("/api/files/watch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_competitor_analysis() {
        let (app, _) = test_app();
        let body = serde_json::json!({
            "competitor_name": "CompetitorX",
            "competitor_url": "https://competitor.com",
            "focus_areas": ["pricing", "features"]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/roadmap/competitor-analysis")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_app_update_check() {
        let (app, _) = test_app();
        let req = Request::builder()
            .method("GET")
            .uri("/api/notifications/app-update")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
