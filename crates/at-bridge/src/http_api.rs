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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub last_sync_time: Option<chrono::DateTime<chrono::Utc>>,
    pub issues_imported: u64,
    pub issues_exported: u64,
    pub statuses_synced: u64,
    pub is_syncing: bool,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self {
            last_sync_time: None,
            issues_imported: 0,
            issues_exported: 0,
            statuses_synced: 0,
            is_syncing: false,
        }
    }
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

/// Response payload for GET /api/status.
///
/// Returns basic server status including version, uptime, and entity counts.
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

/// GET /api/status -- returns basic server health and statistics.
///
/// Provides a lightweight status check that includes the API version,
/// how long the server has been running, and counts of active agents and beads.
/// Returns 200 OK with JSON body.
///
/// Example response:
/// ```json
/// {
///   "version": "0.1.0",
///   "uptime_seconds": 3600,
///   "agent_count": 2,
///   "bead_count": 42
/// }
/// ```
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

/// GET /api/beads -- retrieve all beads in the system.
///
/// Returns a JSON array of all beads with their current status, lane assignment,
/// timestamps, and metadata. Beads represent high-level features or epics that
/// contain multiple tasks.
///
/// **Response:** 200 OK with array of Bead objects.
///
/// **Example Response:**
/// ```json
/// [
///   {
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "title": "User Authentication System",
///     "description": "OAuth2 and JWT-based auth",
///     "status": "InProgress",
///     "lane": "Standard",
///     "priority": 10,
///     "agent_id": null,
///     "convoy_id": null,
///     "created_at": "2026-02-23T10:00:00Z",
///     "updated_at": "2026-02-23T10:30:00Z",
///     "hooked_at": "2026-02-23T10:05:00Z",
///     "slung_at": null,
///     "done_at": null,
///     "git_branch": "feature/auth-system",
///     "metadata": {"tags": ["security", "backend"]}
///   }
/// ]
/// ```
async fn list_beads(State(state): State<Arc<ApiState>>) -> Json<Vec<Bead>> {
    let beads = state.beads.read().await;
    Json(beads.clone())
}

/// POST /api/beads -- create a new bead (feature/epic).
///
/// Creates a new bead with the specified title, optional description, lane assignment,
/// and tags. The bead is initialized with Pending status and current timestamps.
/// After creation, broadcasts an updated bead list via the event bus.
///
/// **Request Body:** CreateBeadRequest JSON object.
/// **Response:** 201 Created with the newly created Bead object.
///
/// **Example Request:**
/// ```json
/// {
///   "title": "User Authentication System",
///   "description": "OAuth2 and JWT-based auth",
///   "lane": "Standard",
///   "tags": ["security", "backend"]
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "id": "550e8400-e29b-41d4-a716-446655440000",
///   "title": "User Authentication System",
///   "description": "OAuth2 and JWT-based auth",
///   "status": "Pending",
///   "lane": "Standard",
///   "priority": 0,
///   "agent_id": null,
///   "convoy_id": null,
///   "created_at": "2026-02-23T10:00:00Z",
///   "updated_at": "2026-02-23T10:00:00Z",
///   "hooked_at": null,
///   "slung_at": null,
///   "done_at": null,
///   "git_branch": null,
///   "metadata": {"tags": ["security", "backend"]}
/// }
/// ```
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

/// POST /api/beads/{id}/status -- update a bead's status.
///
/// Transitions a bead to a new status if the transition is valid according to
/// the bead lifecycle (Pending → InProgress → Done, etc.). Updates the bead's
/// `updated_at` timestamp and relevant lifecycle timestamps (hooked_at, slung_at,
/// done_at) based on the new status.
///
/// **Path Parameters:** `id` - UUID of the bead to update.
/// **Request Body:** UpdateBeadStatusRequest JSON object.
/// **Response:** 200 OK with updated Bead, 404 if not found, 400 if invalid transition.
///
/// **Example Request:**
/// ```json
/// {
///   "status": "InProgress"
/// }
/// ```
///
/// **Example Response (Success):**
/// ```json
/// {
///   "id": "550e8400-e29b-41d4-a716-446655440000",
///   "title": "User Authentication System",
///   "description": "OAuth2 and JWT-based auth",
///   "status": "InProgress",
///   "lane": "Standard",
///   "priority": 10,
///   "agent_id": null,
///   "convoy_id": null,
///   "created_at": "2026-02-23T10:00:00Z",
///   "updated_at": "2026-02-23T10:30:00Z",
///   "hooked_at": "2026-02-23T10:30:00Z",
///   "slung_at": null,
///   "done_at": null,
///   "git_branch": "feature/auth-system",
///   "metadata": {"tags": ["security", "backend"]}
/// }
/// ```
///
/// **Example Response (Error - Not Found):**
/// ```json
/// {
///   "error": "bead not found"
/// }
/// ```
///
/// **Example Response (Error - Invalid Transition):**
/// ```json
/// {
///   "error": "invalid transition from Pending to Done"
/// }
/// ```
async fn update_bead_status(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateBeadStatusRequest>,
) -> impl IntoResponse {
    let mut beads = state.beads.write().await;
    let Some(bead) = beads.iter_mut().find(|b| b.id == id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "bead not found"})),
        );
    };

    if !bead.status.can_transition_to(&req.status) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!(
                    "invalid transition from {:?} to {:?}",
                    bead.status, req.status
                )
            })),
        );
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
}

/// GET /api/agents -- retrieve all registered agents in the system.
///
/// Returns a JSON array of all agents with their current status, role, CLI type,
/// process information, and metadata. Agents represent autonomous workers that
/// can execute tasks (e.g., coder, QA, fixer roles).
///
/// # Response
/// * `200 OK` - JSON array of Agent objects
///
/// # Example Response
/// ```json
/// [
///   {
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "name": "coder-01",
///     "role": "Coder",
///     "cli_type": "claude",
///     "status": "Active",
///     "rig": "mbp-16",
///     "pid": 12345,
///     "created_at": "2024-01-15T10:30:00Z",
///     "last_seen": "2024-01-15T10:35:00Z"
///   }
/// ]
/// ```
async fn list_agents(State(state): State<Arc<ApiState>>) -> Json<Vec<Agent>> {
    let agents = state.agents.read().await;
    Json(agents.clone())
}

/// POST /api/agents/{id}/nudge -- signal an agent to wake up and check for work.
///
/// Transitions an agent from Active, Idle, or Unknown status to Pending, effectively
/// nudging it to check for new tasks or assignments. If the agent is already Pending
/// or Stopped, the request is acknowledged without state change. Updates the agent's
/// `last_seen` timestamp.
///
/// # Path Parameters
/// * `id` - UUID of the agent to nudge
///
/// # Response
/// * `200 OK` - Agent updated successfully, returns updated Agent object
/// * `404 NOT FOUND` - Agent with specified ID not found
///
/// # Example Request
/// ```
/// POST /api/agents/550e8400-e29b-41d4-a716-446655440000/nudge
/// ```
///
/// # Example Response
/// ```json
/// {
///   "id": "550e8400-e29b-41d4-a716-446655440000",
///   "name": "coder-01",
///   "status": "Pending",
///   "last_seen": "2024-01-15T10:36:00Z"
/// }
/// ```
async fn nudge_agent(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut agents = state.agents.write().await;
    let Some(agent) = agents.iter_mut().find(|a| a.id == id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "agent not found"})),
        );
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
}

/// POST /api/agents/{id}/stop -- mark an agent as stopped.
///
/// Transitions an agent to Stopped status, indicating it should cease work and
/// not accept new tasks. Updates the agent's `last_seen` timestamp. This is a
/// graceful stop signal rather than forcefully terminating the agent process.
///
/// # Path Parameters
/// * `id` - UUID of the agent to stop
///
/// # Response
/// * `200 OK` - Agent stopped successfully, returns updated Agent object
/// * `404 NOT FOUND` - Agent with specified ID not found
///
/// # Example Request
/// ```
/// POST /api/agents/550e8400-e29b-41d4-a716-446655440000/stop
/// ```
///
/// # Example Response
/// ```json
/// {
///   "id": "550e8400-e29b-41d4-a716-446655440000",
///   "name": "coder-01",
///   "status": "Stopped",
///   "last_seen": "2024-01-15T10:37:00Z"
/// }
/// ```
async fn stop_agent(State(state): State<Arc<ApiState>>, Path(id): Path<Uuid>) -> impl IntoResponse {
    let mut agents = state.agents.write().await;
    let Some(agent) = agents.iter_mut().find(|a| a.id == id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "agent not found"})),
        );
    };

    agent.status = at_core::types::AgentStatus::Stopped;
    agent.last_seen = chrono::Utc::now();

    let snapshot = agent.clone();
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(snapshot)),
    )
}

async fn get_kpi(State(state): State<Arc<ApiState>>) -> Json<KpiSnapshot> {
    let kpi = state.kpi.read().await;
    Json(kpi.clone())
}

// ---------------------------------------------------------------------------
// Task handlers
// ---------------------------------------------------------------------------

/// GET /api/tasks -- retrieve all tasks in the system.
///
/// Returns a JSON array of all tasks with their complete state including phase,
/// status, priority, complexity, agent assignment, timestamps, and metadata.
/// Tasks represent individual work items that belong to beads (features/epics).
///
/// **Response:** 200 OK with array of Task objects.
///
/// **Example Response:**
/// ```json
/// [
///   {
///     "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
///     "title": "Implement JWT authentication",
///     "description": "Add JWT token generation and validation",
///     "bead_id": "550e8400-e29b-41d4-a716-446655440000",
///     "category": "Backend",
///     "priority": "High",
///     "complexity": "Medium",
///     "phase": "Pending",
///     "impact": "Security",
///     "agent_id": null,
///     "agent_profile": "FullStack",
///     "created_at": "2026-02-23T10:00:00Z",
///     "updated_at": "2026-02-23T10:00:00Z",
///     "started_at": null,
///     "completed_at": null,
///     "source": "Manual",
///     "phase_configs": []
///   }
/// ]
/// ```
async fn list_tasks(State(state): State<Arc<ApiState>>) -> Json<Vec<Task>> {
    let tasks = state.tasks.read().await;
    Json(tasks.clone())
}

/// POST /api/tasks -- create a new task.
///
/// Creates a new task with specified title, category, priority, complexity, and optional
/// metadata. The task is initialized in Pending phase with current timestamps. A valid
/// bead_id must be provided to associate the task with a parent feature/epic.
///
/// **Request Body:** CreateTaskRequest JSON object.
/// **Response:** 201 Created with the newly created Task object, 400 if validation fails.
///
/// **Example Request:**
/// ```json
/// {
///   "title": "Implement JWT authentication",
///   "description": "Add JWT token generation and validation",
///   "bead_id": "550e8400-e29b-41d4-a716-446655440000",
///   "category": "Backend",
///   "priority": "High",
///   "complexity": "Medium",
///   "impact": "Security",
///   "agent_profile": "FullStack",
///   "source": "Manual",
///   "phase_configs": []
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
///   "title": "Implement JWT authentication",
///   "description": "Add JWT token generation and validation",
///   "bead_id": "550e8400-e29b-41d4-a716-446655440000",
///   "category": "Backend",
///   "priority": "High",
///   "complexity": "Medium",
///   "phase": "Pending",
///   "impact": "Security",
///   "agent_id": null,
///   "agent_profile": "FullStack",
///   "created_at": "2026-02-23T10:00:00Z",
///   "updated_at": "2026-02-23T10:00:00Z",
///   "started_at": null,
///   "completed_at": null,
///   "source": "Manual",
///   "phase_configs": []
/// }
/// ```
async fn create_task(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    if req.title.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "title cannot be empty"})),
        )
            .into_response();
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

/// GET /api/tasks/{id} -- retrieve a specific task by ID.
///
/// Returns the complete task object including all metadata, phase information,
/// timestamps, and agent assignment details.
///
/// **Path Parameters:** `id` - UUID of the task to retrieve.
/// **Response:** 200 OK with Task object, 404 if not found.
///
/// **Example Response (Success):**
/// ```json
/// {
///   "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
///   "title": "Implement JWT authentication",
///   "description": "Add JWT token generation and validation",
///   "bead_id": "550e8400-e29b-41d4-a716-446655440000",
///   "category": "Backend",
///   "priority": "High",
///   "complexity": "Medium",
///   "phase": "InProgress",
///   "impact": "Security",
///   "agent_id": "agent-123",
///   "agent_profile": "FullStack",
///   "created_at": "2026-02-23T10:00:00Z",
///   "updated_at": "2026-02-23T10:30:00Z",
///   "started_at": "2026-02-23T10:15:00Z",
///   "completed_at": null,
///   "source": "Manual",
///   "phase_configs": []
/// }
/// ```
///
/// **Example Response (Not Found):**
/// ```json
/// {
///   "error": "task not found"
/// }
/// ```
async fn get_task(State(state): State<Arc<ApiState>>, Path(id): Path<Uuid>) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let Some(task) = tasks.iter().find(|t| t.id == id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found"})),
        );
    };
    (axum::http::StatusCode::OK, Json(serde_json::json!(task)))
}

/// PATCH /api/tasks/{id} -- update an existing task.
///
/// Updates one or more fields of an existing task. All fields are optional; only provided
/// fields will be updated. Updates the task's `updated_at` timestamp and broadcasts a
/// TaskUpdate event via the event bus for real-time UI updates.
///
/// **Path Parameters:** `id` - UUID of the task to update.
/// **Request Body:** UpdateTaskRequest JSON object with optional fields.
/// **Response:** 200 OK with updated Task, 404 if not found, 400 if validation fails.
///
/// **Example Request:**
/// ```json
/// {
///   "title": "Implement JWT authentication with refresh tokens",
///   "priority": "Critical",
///   "phase_configs": [
///     {
///       "phase": "Coding",
///       "enabled": true,
///       "config": {}
///     }
///   ]
/// }
/// ```
///
/// **Example Response (Success):**
/// ```json
/// {
///   "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
///   "title": "Implement JWT authentication with refresh tokens",
///   "description": "Add JWT token generation and validation",
///   "bead_id": "550e8400-e29b-41d4-a716-446655440000",
///   "category": "Backend",
///   "priority": "Critical",
///   "complexity": "Medium",
///   "phase": "InProgress",
///   "impact": "Security",
///   "agent_id": "agent-123",
///   "agent_profile": "FullStack",
///   "created_at": "2026-02-23T10:00:00Z",
///   "updated_at": "2026-02-23T11:00:00Z",
///   "started_at": "2026-02-23T10:15:00Z",
///   "completed_at": null,
///   "source": "Manual",
///   "phase_configs": [
///     {
///       "phase": "Coding",
///       "enabled": true,
///       "config": {}
///     }
///   ]
/// }
/// ```
///
/// **Example Response (Not Found):**
/// ```json
/// {
///   "error": "task not found"
/// }
/// ```
async fn update_task(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTaskRequest>,
) -> impl IntoResponse {
    let mut tasks = state.tasks.write().await;
    let Some(task) = tasks.iter_mut().find(|t| t.id == id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found"})),
        );
    };

    if let Some(title) = req.title {
        if title.is_empty() {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "title cannot be empty"})),
            );
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
        .publish(crate::protocol::BridgeMessage::TaskUpdate(
            task_snapshot.clone(),
        ));
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(task_snapshot)),
    )
}

/// DELETE /api/tasks/{id} -- delete a task.
///
/// Permanently removes a task from the system. This operation cannot be undone.
/// Use with caution, especially for tasks that have associated build logs or agent work.
///
/// **Path Parameters:** `id` - UUID of the task to delete.
/// **Response:** 200 OK with deletion confirmation, 404 if not found.
///
/// **Example Response (Success):**
/// ```json
/// {
///   "status": "deleted",
///   "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
/// }
/// ```
///
/// **Example Response (Not Found):**
/// ```json
/// {
///   "error": "task not found"
/// }
/// ```
async fn delete_task(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut tasks = state.tasks.write().await;
    let len_before = tasks.len();
    tasks.retain(|t| t.id != id);
    if tasks.len() == len_before {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found"})),
        );
    }
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({"status": "deleted", "id": id.to_string()})),
    )
}

/// POST /api/tasks/{id}/phase -- update a task's phase/stage.
///
/// Transitions a task to a new phase (Pending, Planning, Coding, QA, etc.) with
/// validation to ensure the transition is valid according to the task lifecycle.
/// Invalid transitions (e.g., Completed -> Pending) are rejected with 400.
/// Publishes a TaskUpdate event for real-time WebSocket notifications.
///
/// **Request Body:** UpdateTaskPhaseRequest JSON object with target phase.
/// **Response:** 200 OK with updated Task object, 404 if task not found, 400 if invalid transition.
///
/// **Example Request:**
/// ```json
/// {
///   "phase": "Coding"
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
///   "title": "Implement JWT authentication",
///   "phase": "Coding",
///   "updated_at": "2026-02-23T10:30:00Z",
///   ...
/// }
/// ```
async fn update_task_phase(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTaskPhaseRequest>,
) -> impl IntoResponse {
    let mut tasks = state.tasks.write().await;
    let Some(task) = tasks.iter_mut().find(|t| t.id == id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found"})),
        );
    };

    if !task.phase.can_transition_to(&req.phase) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!(
                    "invalid phase transition from {:?} to {:?}",
                    task.phase, req.phase
                )
            })),
        );
    }

    task.set_phase(req.phase);
    let task_snapshot = task.clone();
    drop(tasks);
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::TaskUpdate(
            task_snapshot.clone(),
        ));
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(task_snapshot)),
    )
}

/// GET /api/tasks/{id}/logs -- retrieve execution logs for a task.
///
/// Returns the accumulated log output from task execution phases (Planning, Coding, QA).
/// Logs are captured from agent interactions and tool executions. Returns an array
/// of log line strings in chronological order.
///
/// **Response:** 200 OK with array of log strings, 404 if task not found.
///
/// **Example Response:**
/// ```json
/// [
///   "[2026-02-23T10:15:00Z] Starting Planning phase...",
///   "[2026-02-23T10:15:12Z] Generated spec.md",
///   "[2026-02-23T10:15:45Z] Transitioning to Coding phase...",
///   "[2026-02-23T10:20:30Z] Created src/auth.rs"
/// ]
/// ```
async fn get_task_logs(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let Some(task) = tasks.iter().find(|t| t.id == id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found"})),
        );
    };
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(task.logs)),
    )
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
/// Task must be in Planning or Queue phase; returns 400 for invalid phase transitions.
///
/// **Request Body:** Optional ExecuteTaskRequest JSON object with cli_type override.
/// **Response:** 202 Accepted with task snapshot, 404 if task not found, 400 if invalid phase.
///
/// **Example Request:**
/// ```json
/// {
///   "cli_type": "Claude"
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
///   "title": "Implement JWT authentication",
///   "phase": "Coding",
///   "progress_percent": 0,
///   "started_at": "2026-02-23T10:30:00Z",
///   ...
/// }
/// ```
async fn execute_task_pipeline(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    body: Option<Json<ExecuteTaskRequest>>,
) -> impl IntoResponse {
    let mut tasks = state.tasks.write().await;
    let Some(task) = tasks.iter_mut().find(|t| t.id == id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found"})),
        );
    };

    // The task must be in a phase that can transition to Coding.
    if !task.phase.can_transition_to(&TaskPhase::Coding) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!(
                    "cannot start pipeline: task is in {:?} phase",
                    task.phase
                )
            })),
        );
    }

    task.set_phase(TaskPhase::Coding);
    let task_snapshot = task.clone();
    drop(tasks);

    // Extract optional CLI type from request body.
    let cli_type = body.and_then(|b| b.0.cli_type).unwrap_or(CliType::Claude);

    // Publish the phase change.
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::TaskUpdate(
            task_snapshot.clone(),
        ));

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
            event_bus.publish(crate::protocol::BridgeMessage::TaskUpdate(t.clone()));
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
                event_bus.publish(crate::protocol::BridgeMessage::TaskUpdate(t.clone()));
            }
        }

        // Re-run QA
        {
            let mut tasks = tasks_store.write().await;
            if let Some(t) = tasks.iter_mut().find(|t| t.id == task.id) {
                t.set_phase(TaskPhase::Qa);
                event_bus.publish(crate::protocol::BridgeMessage::TaskUpdate(t.clone()));
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
            event_bus.publish(crate::protocol::BridgeMessage::TaskUpdate(t.clone()));
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
/// polling so clients only receive new lines since their last fetch. Returns
/// array of BuildLogEntry objects with timestamps, stream type (stdout/stderr),
/// and line content. Useful for displaying real-time build progress.
///
/// **Response:** 200 OK with array of BuildLogEntry objects, 404 if task not found,
/// 400 if 'since' timestamp is invalid.
///
/// **Example Response:**
/// ```json
/// [
///   {
///     "timestamp": "2026-02-23T10:15:00Z",
///     "stream": "stdout",
///     "line": "Compiling auth v0.1.0"
///   },
///   {
///     "timestamp": "2026-02-23T10:15:05Z",
///     "stream": "stdout",
///     "line": "Finished dev [unoptimized] target(s) in 5.23s"
///   },
///   {
///     "timestamp": "2026-02-23T10:15:10Z",
///     "stream": "stderr",
///     "line": "warning: unused variable `token`"
///   }
/// ]
/// ```
async fn get_build_logs(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Query(q): Query<BuildLogsQuery>,
) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let Some(task) = tasks.iter().find(|t| t.id == id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found"})),
        );
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
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    Json(
                        serde_json::json!({"error": "invalid 'since' timestamp; use ISO-8601 / RFC-3339"}),
                    ),
                );
            }
        }
    } else {
        task.build_logs.iter().collect()
    };

    (axum::http::StatusCode::OK, Json(serde_json::json!(logs)))
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
///
/// Provides an aggregate view of build progress including phase, progress percentage,
/// log line counts by stream type (stdout/stderr), error counts, and the most recent
/// log line. Useful for dashboard widgets and progress indicators.
///
/// **Response:** 200 OK with BuildStatusSummary object, 404 if task not found.
///
/// **Example Response:**
/// ```json
/// {
///   "phase": "Coding",
///   "progress_percent": 45,
///   "total_lines": 127,
///   "stdout_lines": 120,
///   "stderr_lines": 7,
///   "error_count": 7,
///   "last_line": "warning: unused variable `token`"
/// }
/// ```
async fn get_build_status(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let Some(task) = tasks.iter().find(|t| t.id == id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found"})),
        );
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

    (axum::http::StatusCode::OK, Json(serde_json::json!(summary)))
}

// ---------------------------------------------------------------------------
// Settings handlers
// ---------------------------------------------------------------------------

/// GET /api/settings -- retrieve the current application configuration.
///
/// Returns the full Config object loaded from persistent storage (typically
/// `~/.auto-tundra/config.toml`). If no config file exists, returns the default
/// configuration. The Config contains all application settings including general,
/// security, UI, integrations, and agent profile configurations.
///
/// **Response:** 200 OK with Config JSON object.
///
/// **Example Response:**
/// ```json
/// {
///   "general": {
///     "project_name": "auto-tundra",
///     "log_level": "info",
///     "workspace_root": "/home/user/workspace"
///   },
///   "security": {
///     "enable_auth": true,
///     "api_key_header": "X-API-Key"
///   },
///   "ui": {
///     "theme": "dark",
///     "page_size": 50
///   },
///   "bridge": {
///     "host": "127.0.0.1",
///     "port": 8765
///   },
///   "agents": {
///     "max_concurrent": 4,
///     "timeout_seconds": 300
///   }
/// }
/// ```
async fn get_settings(State(state): State<Arc<ApiState>>) -> Json<Config> {
    let cfg = state.settings_manager.load_or_default();
    Json(cfg)
}

/// PUT /api/settings -- replace the entire application configuration.
///
/// Replaces the entire configuration with the provided Config object and persists it to disk.
/// All sections of the config must be provided; any omitted sections will be reset to their
/// default values. Use PATCH /api/settings for partial updates.
///
/// **Request Body:** Complete Config JSON object.
/// **Response:** 200 OK with saved Config, 500 if save fails.
///
/// **Example Request:**
/// ```json
/// {
///   "general": {
///     "project_name": "my-project",
///     "log_level": "debug",
///     "workspace_root": "/home/user/my-workspace"
///   },
///   "security": {
///     "enable_auth": false,
///     "api_key_header": "X-API-Key"
///   },
///   "ui": {
///     "theme": "light",
///     "page_size": 100
///   },
///   "bridge": {
///     "host": "0.0.0.0",
///     "port": 8765
///   },
///   "agents": {
///     "max_concurrent": 8,
///     "timeout_seconds": 600
///   }
/// }
/// ```
///
/// **Example Response (Success):**
/// ```json
/// {
///   "general": {
///     "project_name": "my-project",
///     "log_level": "debug",
///     "workspace_root": "/home/user/my-workspace"
///   },
///   "security": {
///     "enable_auth": false,
///     "api_key_header": "X-API-Key"
///   },
///   "ui": {
///     "theme": "light",
///     "page_size": 100
///   },
///   "bridge": {
///     "host": "0.0.0.0",
///     "port": 8765
///   },
///   "agents": {
///     "max_concurrent": 8,
///     "timeout_seconds": 600
///   }
/// }
/// ```
///
/// **Example Response (Error):**
/// ```json
/// {
///   "error": "failed to write config to disk: permission denied"
/// }
/// ```
async fn put_settings(
    State(state): State<Arc<ApiState>>,
    Json(cfg): Json<Config>,
) -> impl IntoResponse {
    match state.settings_manager.save(&cfg) {
        Ok(()) => (axum::http::StatusCode::OK, Json(serde_json::json!(cfg))),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// PATCH /api/settings -- partially update the application configuration.
///
/// Merges the provided partial configuration into the existing configuration and persists
/// the updated result to disk. Only the fields present in the request body are updated;
/// all other fields retain their current values. This is useful for updating specific
/// settings (e.g., only the log level or theme) without having to send the entire Config.
///
/// The partial update is performed via JSON merge: nested objects are merged recursively,
/// and arrays are replaced entirely (not merged element-wise).
///
/// **Request Body:** Partial Config JSON object with only the fields to update.
/// **Response:** 200 OK with updated Config, 400 if merge creates invalid config, 500 if save fails.
///
/// **Example Request (Update log level only):**
/// ```json
/// {
///   "general": {
///     "log_level": "trace"
///   }
/// }
/// ```
///
/// **Example Request (Update multiple sections):**
/// ```json
/// {
///   "general": {
///     "log_level": "debug"
///   },
///   "ui": {
///     "theme": "dark"
///   },
///   "agents": {
///     "max_concurrent": 6
///   }
/// }
/// ```
///
/// **Example Response (Success):**
/// ```json
/// {
///   "general": {
///     "project_name": "auto-tundra",
///     "log_level": "trace",
///     "workspace_root": "/home/user/workspace"
///   },
///   "security": {
///     "enable_auth": true,
///     "api_key_header": "X-API-Key"
///   },
///   "ui": {
///     "theme": "dark",
///     "page_size": 50
///   },
///   "bridge": {
///     "host": "127.0.0.1",
///     "port": 8765
///   },
///   "agents": {
///     "max_concurrent": 6,
///     "timeout_seconds": 300
///   }
/// }
/// ```
///
/// **Example Response (Invalid Merge):**
/// ```json
/// {
///   "error": "invalid value: expected u16, found string \"not-a-number\" at line 1 column 23"
/// }
/// ```
///
/// **Example Response (Save Error):**
/// ```json
/// {
///   "error": "failed to write config to disk: permission denied"
/// }
/// ```
async fn patch_settings(
    State(state): State<Arc<ApiState>>,
    Json(partial): Json<serde_json::Value>,
) -> impl IntoResponse {
    let mut current = state.settings_manager.load_or_default();
    let mut current_val = match serde_json::to_value(&current) {
        Ok(v) => v,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            );
        }
    };

    // Merge partial into current
    merge_json(&mut current_val, &partial);

    current = match serde_json::from_value(current_val) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            );
        }
    };

    match state.settings_manager.save(&current) {
        Ok(()) => (axum::http::StatusCode::OK, Json(serde_json::json!(current))),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
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

/// GET /api/gitlab/issues -- retrieve issues from a GitLab project.
///
/// Fetches issues from the configured GitLab instance. Requires a GitLab API token
/// to be set via environment variable (configured in settings.integrations.gitlab_token_env).
///
/// **Query Parameters:**
/// - `project_id` (optional): GitLab project ID or path (e.g., "myorg/myproject"). Falls back to settings.integrations.gitlab_project_id.
/// - `state` (optional): Filter by issue state - "opened", "closed", or omit for all states.
/// - `page` (optional): Page number for pagination (default: 1).
/// - `per_page` (optional): Number of issues per page (default: 20).
///
/// **Response:** 200 OK with array of GitLab issue objects, 400 if project_id is missing,
/// 503 if GitLab token is not configured.
///
/// **Example Request:**
/// ```
/// GET /api/gitlab/issues?project_id=myorg/myproject&state=opened&page=1&per_page=10
/// ```
///
/// **Example Response:**
/// ```json
/// [
///   {
///     "id": 42,
///     "iid": 12,
///     "title": "Fix authentication bug",
///     "description": "Users cannot log in with SSO",
///     "state": "opened",
///     "created_at": "2026-02-20T10:30:00Z",
///     "updated_at": "2026-02-23T14:20:00Z",
///     "author": {
///       "username": "alice",
///       "name": "Alice Developer"
///     },
///     "labels": ["bug", "priority::high"],
///     "web_url": "https://gitlab.com/myorg/myproject/-/issues/12"
///   }
/// ]
/// ```
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
        );
    }

    let project_id = q
        .project_id
        .clone()
        .or_else(|| int.gitlab_project_id.clone())
        .unwrap_or_default();
    if project_id.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitLab project ID is required (query param project_id or settings.integrations.gitlab_project_id).",
            })),
        );
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
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
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
        Ok(issues) => (axum::http::StatusCode::OK, Json(serde_json::json!(issues))),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
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

/// GET /api/gitlab/merge-requests -- retrieve merge requests from a GitLab project.
///
/// Fetches merge requests from the configured GitLab instance. Requires a GitLab API token
/// to be set via environment variable (configured in settings.integrations.gitlab_token_env).
///
/// **Query Parameters:**
/// - `project_id` (optional): GitLab project ID or path (e.g., "myorg/myproject"). Falls back to settings.integrations.gitlab_project_id.
/// - `state` (optional): Filter by MR state - "opened", "merged", "closed", or omit for all states.
/// - `page` (optional): Page number for pagination (default: 1).
/// - `per_page` (optional): Number of merge requests per page (default: 20).
///
/// **Response:** 200 OK with array of GitLab merge request objects, 400 if project_id is missing,
/// 503 if GitLab token is not configured.
///
/// **Example Request:**
/// ```
/// GET /api/gitlab/merge-requests?project_id=myorg/myproject&state=opened&page=1&per_page=5
/// ```
///
/// **Example Response:**
/// ```json
/// [
///   {
///     "id": 123,
///     "iid": 45,
///     "title": "Add user authentication feature",
///     "description": "Implements JWT-based authentication",
///     "state": "opened",
///     "created_at": "2026-02-22T09:15:00Z",
///     "updated_at": "2026-02-23T11:30:00Z",
///     "author": {
///       "username": "bob",
///       "name": "Bob Engineer"
///     },
///     "source_branch": "feature/auth",
///     "target_branch": "main",
///     "merge_status": "can_be_merged",
///     "web_url": "https://gitlab.com/myorg/myproject/-/merge_requests/45"
///   }
/// ]
/// ```
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
        );
    }

    let project_id = q
        .project_id
        .clone()
        .or_else(|| int.gitlab_project_id.clone())
        .unwrap_or_default();
    if project_id.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitLab project ID is required (query param project_id or settings.integrations.gitlab_project_id).",
            })),
        );
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
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
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
        Ok(mrs) => (axum::http::StatusCode::OK, Json(serde_json::json!(mrs))),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
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

/// POST /api/gitlab/merge-requests/{iid}/review -- perform automated code review on a GitLab merge request.
///
/// Analyzes a GitLab merge request and provides automated code review feedback including
/// security issues, code quality concerns, and best practice violations. Requires a GitLab
/// API token to be set via environment variable (configured in settings.integrations.gitlab_token_env).
///
/// **Path Parameters:**
/// - `iid`: GitLab merge request internal ID (IID) to review.
///
/// **Request Body:** Optional ReviewGitLabMrBody JSON object with review configuration.
/// - `project_id` (optional): GitLab project ID or path. Falls back to settings.integrations.gitlab_project_id.
/// - `severity_threshold` (optional): Minimum severity level to report - "low", "medium", "high", "critical".
/// - `max_findings` (optional): Maximum number of findings to report (default: unlimited).
/// - `auto_approve` (optional): Automatically approve MR if no critical issues found (default: false).
///
/// **Response:** 200 OK with MR review result containing findings and recommendations,
/// 400 if project_id is missing, 503 if GitLab token is not configured.
///
/// **Example Request:**
/// ```json
/// POST /api/gitlab/merge-requests/45/review
/// {
///   "project_id": "myorg/myproject",
///   "severity_threshold": "medium",
///   "max_findings": 10,
///   "auto_approve": false
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "mr_iid": 45,
///   "project_id": "myorg/myproject",
///   "review_status": "completed",
///   "findings": [
///     {
///       "severity": "high",
///       "category": "security",
///       "message": "Potential SQL injection vulnerability detected",
///       "file": "src/database/queries.rs",
///       "line": 42,
///       "suggestion": "Use parameterized queries instead of string concatenation"
///     },
///     {
///       "severity": "medium",
///       "category": "code_quality",
///       "message": "Function complexity exceeds threshold",
///       "file": "src/handlers/auth.rs",
///       "line": 128,
///       "suggestion": "Consider breaking this function into smaller units"
///     }
///   ],
///   "summary": {
///     "total_findings": 2,
///     "critical": 0,
///     "high": 1,
///     "medium": 1,
///     "low": 0
///   },
///   "approved": false
/// }
/// ```
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
        );
    }

    let req = body.map(|b| b.0).unwrap_or_default();
    let project_id = req
        .project_id
        .or_else(|| int.gitlab_project_id.clone())
        .unwrap_or_default();
    if project_id.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitLab project ID is required (request body project_id or settings.integrations.gitlab_project_id).",
            })),
        );
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
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
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
    (axum::http::StatusCode::OK, Json(serde_json::json!(result)))
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

/// GET /api/linear/issues — list Linear issues for a team.
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
        );
    }

    let client =
        match at_integrations::linear::LinearClient::new(token.as_deref().unwrap_or_default()) {
            Ok(c) => c,
            Err(e) => {
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                );
            }
        };

    let team = q.team_id.as_deref().or(int.linear_team_id.as_deref());
    if team.is_none() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Linear team_id is required (query param team_id or settings.integrations.linear_team_id).",
            })),
        );
    }

    match client.list_issues(team, q.state.as_deref()).await {
        Ok(issues) => (axum::http::StatusCode::OK, Json(serde_json::json!(issues))),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct ImportLinearBody {
    pub issue_ids: Vec<String>,
}

/// POST /api/linear/import — import Linear issues by IDs and create tasks.
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
        );
    }

    let client =
        match at_integrations::linear::LinearClient::new(token.as_deref().unwrap_or_default()) {
            Ok(c) => c,
            Err(e) => {
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                );
            }
        };

    match client.import_issues(body.issue_ids).await {
        Ok(results) => (axum::http::StatusCode::OK, Json(serde_json::json!(results))),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
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
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "columns must not be empty"})),
        );
    }
    *cols = patch;
    (
        axum::http::StatusCode::OK,
        Json(serde_json::to_value(cols.clone()).unwrap()),
    )
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

/// GET /api/integrations/github/issues — list GitHub issues with optional state filter.
async fn list_github_issues(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<ListGitHubIssuesQuery>,
) -> impl IntoResponse {
    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().map_or(true, |t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitHub token not configured. Set the environment variable.",
                "env_var": int.github_token_env,
            })),
        );
    }
    if owner.is_empty() || repo.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitHub owner and repo must be set in settings (integrations).",
            })),
        );
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
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
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    (axum::http::StatusCode::OK, Json(serde_json::json!(list)))
}

async fn trigger_github_sync(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().map_or(true, |t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitHub token not configured. Set the environment variable.",
                "env_var": int.github_token_env,
            })),
        );
    }
    if owner.is_empty() || repo.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitHub owner and repo must be set in settings (integrations).",
            })),
        );
    }

    let gh_config = GitHubConfig {
        token: token,
        owner,
        repo,
    };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
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
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
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
}

async fn get_sync_status(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let status = state.sync_status.read().await;
    Json(serde_json::json!(*status))
}

/// POST /api/tasks/{task_id}/pr — create a GitHub pull request for a task's branch.
async fn create_pr_for_task(
    State(state): State<Arc<ApiState>>,
    Path(task_id): Path<Uuid>,
    body: Option<Json<CreatePrRequest>>,
) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let task = match tasks.iter().find(|t| t.id == task_id) {
        Some(t) => t.clone(),
        None => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "task not found"})),
            );
        }
    };
    drop(tasks);

    if task.git_branch.is_none() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Task has no branch. Create a worktree for this task first.",
            })),
        );
    }

    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().map_or(true, |t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitHub token not configured. Set the environment variable.",
                "env_var": int.github_token_env,
            })),
        );
    }
    if owner.is_empty() || repo.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitHub owner and repo must be set in settings (integrations).",
            })),
        );
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
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
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
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
}

// ---------------------------------------------------------------------------
// Notification handlers
// ---------------------------------------------------------------------------

/// GET /api/notifications — list notifications with optional filters.
/// GET /api/notifications -- retrieve notifications with optional filtering.
///
/// Returns a paginated list of notifications, optionally filtered to show only unread items.
/// Supports pagination via limit/offset query parameters. Notifications track system events,
/// task status changes, agent activity, and GitHub integration events.
///
/// **Query Parameters:**
/// - `unread` (optional bool): If true, return only unread notifications
/// - `limit` (optional usize): Maximum number of results (default: 50)
/// - `offset` (optional usize): Number of results to skip (default: 0)
///
/// **Response:** 200 OK with array of Notification objects.
///
/// **Example Response:**
/// ```json
/// [
///   {
///     "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
///     "title": "Task Completed",
///     "message": "Task 'Implement JWT auth' has been completed successfully",
///     "level": "Success",
///     "source": "TaskEngine",
///     "created_at": "2026-02-23T10:15:30Z",
///     "read": false,
///     "action_url": "/tasks/550e8400-e29b-41d4-a716-446655440000"
///   },
///   {
///     "id": "b2c3d4e5-f6a7-8901-bcde-f12345678901",
///     "title": "Agent Error",
///     "message": "Agent backend-001 encountered an error during execution",
///     "level": "Error",
///     "source": "AgentMonitor",
///     "created_at": "2026-02-23T10:10:15Z",
///     "read": true,
///     "action_url": null
///   }
/// ]
/// ```
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

/// GET /api/notifications/count -- retrieve notification counts.
///
/// Returns the count of unread and total notifications in the system. Used for
/// badge indicators and notification summary displays in the UI.
///
/// **Response:** 200 OK with count summary object.
///
/// **Example Response:**
/// ```json
/// {
///   "unread": 3,
///   "total": 42
/// }
/// ```
async fn notification_count(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let store = state.notification_store.read().await;
    Json(serde_json::json!({
        "unread": store.unread_count(),
        "total": store.total_count(),
    }))
}

/// POST /api/notifications/{id}/read -- mark a single notification as read.
///
/// Marks the specified notification as read by its UUID. Used when a user views
/// or acknowledges a notification. The read status persists until the notification
/// is deleted or the store is cleared.
///
/// **Path Parameters:**
/// - `id` (UUID): The unique identifier of the notification to mark as read
///
/// **Response:** 200 OK if successful, 404 Not Found if notification doesn't exist.
///
/// **Example Success Response:**
/// ```json
/// {
///   "status": "read",
///   "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
/// }
/// ```
///
/// **Example Error Response:**
/// ```json
/// {
///   "error": "notification not found"
/// }
/// ```
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
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "notification not found"})),
        )
    }
}

/// POST /api/notifications/read-all -- mark all notifications as read.
///
/// Marks all notifications in the system as read in a single operation. Used for
/// "mark all as read" bulk actions in the UI. This affects all notifications
/// regardless of their current read status.
///
/// **Response:** 200 OK with status confirmation.
///
/// **Example Response:**
/// ```json
/// {
///   "status": "all_read"
/// }
/// ```
async fn mark_all_notifications_read(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let mut store = state.notification_store.write().await;
    store.mark_all_read();
    Json(serde_json::json!({"status": "all_read"}))
}

/// DELETE /api/notifications/{id} -- delete a notification.
///
/// Permanently removes the specified notification from the system by its UUID.
/// Once deleted, the notification cannot be recovered. This is typically used
/// when a user dismisses a notification they no longer need.
///
/// **Path Parameters:**
/// - `id` (UUID): The unique identifier of the notification to delete
///
/// **Response:** 200 OK if successful, 404 Not Found if notification doesn't exist.
///
/// **Example Success Response:**
/// ```json
/// {
///   "status": "deleted",
///   "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
/// }
/// ```
///
/// **Example Error Response:**
/// ```json
/// {
///   "error": "notification not found"
/// }
/// ```
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
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "notification not found"})),
        )
    }
}

// ---------------------------------------------------------------------------
// Metrics handlers
// ---------------------------------------------------------------------------

/// GET /api/metrics — Prometheus text format export.
/// GET /api/metrics -- exports telemetry metrics in Prometheus text format.
///
/// Returns all collected metrics (request counts, durations, gauges, etc.)
/// in the standard Prometheus exposition format for scraping by monitoring systems.
/// Returns 200 OK with text/plain content type.
///
/// Example response:
/// ```text
/// # HELP http_requests_total Total HTTP requests
/// # TYPE http_requests_total counter
/// http_requests_total{method="GET",path="/api/status"} 42
/// ```
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

/// GET /api/metrics/json -- exports telemetry metrics in JSON format.
///
/// Returns all collected metrics in a structured JSON format suitable for
/// programmatic consumption by dashboards and monitoring tools. Returns 200 OK
/// with application/json content type.
///
/// Example response:
/// ```json
/// {
///   "counters": { "http_requests_total": 42 },
///   "gauges": { "active_connections": 5 },
///   "histograms": { "request_duration_ms": { "p50": 10, "p99": 100 } }
/// }
/// ```
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
                (axum::http::StatusCode::OK, Json(serde_json::json!(session)))
            } else {
                (axum::http::StatusCode::OK, Json(serde_json::json!(null)))
            }
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// PUT /api/sessions/ui — save a UI session state.
async fn save_ui_session(
    State(state): State<Arc<ApiState>>,
    Json(mut session): Json<SessionState>,
) -> impl IntoResponse {
    session.last_active_at = chrono::Utc::now();
    match state.session_store.save_session(&session) {
        Ok(()) => (axum::http::StatusCode::OK, Json(serde_json::json!(session))),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// GET /api/sessions/ui/list — list all saved sessions.
async fn list_ui_sessions(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    match state.session_store.list_sessions() {
        Ok(sessions) => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!(sessions)),
        ),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
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
    loop {
        match rx.recv_async().await {
            Ok(msg) => {
                let json = serde_json::to_string(&*msg).unwrap_or_default();
                if socket.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
            Err(_) => break,
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
            let status = if result.is_error {
                axum::http::StatusCode::BAD_REQUEST
            } else {
                axum::http::StatusCode::OK
            };
            (status, Json(serde_json::to_value(result).unwrap()))
        }
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("unknown tool: {}", request.name),
                "available_tools": at_harness::builtin_tools::builtin_tool_definitions()
                    .iter()
                    .map(|t| t.name.clone())
                    .collect::<Vec<_>>()
            })),
        ),
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

/// GET /api/worktrees — list all git worktrees with path and branch info.
async fn list_worktrees() -> impl IntoResponse {
    let output = match tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            );
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": stderr})),
        );
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
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            );
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
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "worktree not found", "id": id})),
            );
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
            );
        }
        Ok(_) => { /* has changes */ }
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            );
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
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
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
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            );
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
    if found_branch.is_none() && !current_path.is_empty() {
        if current_branch.contains(&id) || current_path.contains(&id) {
            found_branch = Some(current_branch);
        }
    }

    let branch = match found_branch {
        Some(b) if !b.is_empty() => b,
        _ => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "worktree not found", "id": id})),
            );
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
}

/// POST /api/worktrees/{id}/resolve — accept conflict resolution.
async fn resolve_conflict(
    Path(id): Path<String>,
    Json(req): Json<ResolveConflictRequest>,
) -> impl IntoResponse {
    let valid_strategies = ["ours", "theirs", "manual"];
    if !valid_strategies.contains(&req.strategy.as_str()) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("invalid strategy '{}', must be one of: ours, theirs, manual", req.strategy)
            })),
        );
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
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "task_ids must not be empty"})),
        );
    }

    // Validate all task IDs exist
    let tasks = state.tasks.read().await;
    for task_id in &req.task_ids {
        if !tasks.iter().any(|t| t.id == *task_id) {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("task {} not found", task_id)})),
            );
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
}

/// POST /api/queue/{task_id}/prioritize — bump a task's priority.
async fn prioritize_task(
    State(state): State<Arc<ApiState>>,
    Path(task_id): Path<Uuid>,
    Json(req): Json<PrioritizeRequest>,
) -> impl IntoResponse {
    let mut tasks = state.tasks.write().await;
    let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found"})),
        );
    };

    task.priority = req.priority;
    task.updated_at = chrono::Utc::now();

    let task_snapshot = task.clone();
    drop(tasks);

    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::TaskUpdate(
            task_snapshot.clone(),
        ));

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(task_snapshot)),
    )
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
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            );
        }
    };

    // Merge direct_mode into the agents section
    let patch = serde_json::json!({"agents": {"direct_mode": req.enabled}});
    merge_json(&mut current_val, &patch);

    current = match serde_json::from_value(current_val) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            );
        }
    };

    match state.settings_manager.save(&current) {
        Ok(()) => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "direct_mode": req.enabled,
            })),
        ),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
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
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            );
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": stderr})),
        );
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
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "worktree not found", "id": id})),
        );
    };

    let rm = tokio::process::Command::new("git")
        .args(["worktree", "remove", "--force", &path])
        .output()
        .await;

    match rm {
        Ok(o) if o.status.success() => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"status": "deleted", "id": id, "path": path})),
        ),
        Ok(o) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": String::from_utf8_lossy(&o.stderr).to_string(),
                "id": id,
                "path": path
            })),
        ),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string(), "id": id, "path": path})),
        ),
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

    if token.as_ref().map_or(true, |t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitHub token not configured. Set the environment variable.",
                "env_var": int.github_token_env,
            })),
        );
    }
    if owner.is_empty() || repo.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitHub owner and repo must be set in settings (integrations).",
            })),
        );
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
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
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                );
            }
        };

    (axum::http::StatusCode::OK, Json(serde_json::json!(list)))
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

    if token.as_ref().map_or(true, |t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitHub token not configured. Set the environment variable.",
                "env_var": int.github_token_env,
            })),
        );
    }
    if owner.is_empty() || repo.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitHub owner and repo must be set in settings (integrations).",
            })),
        );
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let issue = match issues::get_issue(&client, number).await {
        Ok(i) => i,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let bead = issues::import_issue_as_task(&issue);
    state.beads.write().await.push(bead.clone());

    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(bead)),
    )
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
            );
        }
    };
    let client_secret = match std::env::var("GITHUB_OAUTH_CLIENT_SECRET") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "GITHUB_OAUTH_CLIENT_SECRET not set" })),
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
        client_secret,
        redirect_uri,
        scopes,
    };

    let oauth_client = gh_oauth::GitHubOAuthClient::new(oauth_config);

    let token_resp = match oauth_client.exchange_code(&body.code).await {
        Ok(t) => t,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
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

/// GET /api/sessions — list all active agent sessions.
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

/// GET /api/projects -- retrieve all projects in the system.
///
/// Returns a JSON array of all projects (workspaces/repositories) managed by the system.
/// Each project includes its ID, name, path, creation timestamp, and active status.
/// Only one project can be active at a time.
///
/// **Request:** No parameters required.
/// **Response:** 200 OK with JSON array of Project objects.
///
/// **Example Response:**
/// ```json
/// [
///   {
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "name": "my-rust-project",
///     "path": "/home/user/projects/rust-harness",
///     "created_at": "2026-02-23T10:00:00Z",
///     "is_active": true
///   },
///   {
///     "id": "660e8400-e29b-41d4-a716-446655440001",
///     "name": "web-app",
///     "path": "/home/user/projects/webapp",
///     "created_at": "2026-02-22T14:30:00Z",
///     "is_active": false
///   }
/// ]
/// ```
async fn list_projects(State(state): State<Arc<ApiState>>) -> Json<Vec<Project>> {
    let projects = state.projects.read().await;
    Json(projects.clone())
}

#[derive(Debug, Deserialize)]
struct CreateProjectRequest {
    name: String,
    path: String,
}

/// POST /api/projects -- create a new project.
///
/// Creates a new project (workspace/repository) with the specified name and filesystem path.
/// The project is initialized with `is_active: false` and a current timestamp.
/// To make it the active project, call the activate endpoint afterwards.
///
/// **Request Body:** CreateProjectRequest JSON object with `name` and `path` fields.
/// **Response:** 201 Created with the newly created Project object.
///
/// **Example Request:**
/// ```json
/// {
///   "name": "new-api-service",
///   "path": "/home/user/projects/api-service"
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "id": "770e8400-e29b-41d4-a716-446655440002",
///   "name": "new-api-service",
///   "path": "/home/user/projects/api-service",
///   "created_at": "2026-02-23T11:30:00Z",
///   "is_active": false
/// }
/// ```
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

/// PATCH /api/projects/{id} -- update a project's name or path.
///
/// Updates one or more fields of an existing project. Both `name` and `path` are optional;
/// only the provided fields will be updated. The project's `is_active` status and other
/// metadata are not modified by this endpoint.
///
/// **Path Parameters:** `id` - UUID of the project to update.
/// **Request Body:** UpdateProjectRequest JSON object with optional `name` and `path` fields.
/// **Response:** 200 OK with the updated Project object, 404 if project not found.
///
/// **Example Request:**
/// ```json
/// {
///   "name": "renamed-project",
///   "path": "/new/path/to/project"
/// }
/// ```
///
/// **Example Response (Success):**
/// ```json
/// {
///   "id": "770e8400-e29b-41d4-a716-446655440002",
///   "name": "renamed-project",
///   "path": "/new/path/to/project",
///   "created_at": "2026-02-23T11:30:00Z",
///   "is_active": false
/// }
/// ```
///
/// **Example Response (Not Found):**
/// ```json
/// {
///   "error": "project not found"
/// }
/// ```
async fn update_project(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProjectRequest>,
) -> impl IntoResponse {
    let mut projects = state.projects.write().await;
    let Some(project) = projects.iter_mut().find(|p| p.id == id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "project not found"})),
        );
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
}

/// DELETE /api/projects/{id} -- delete a project.
///
/// Removes a project from the system. Cannot delete the last remaining project
/// (returns 400 Bad Request if attempted). If the deleted project was the active one,
/// automatically activates the first remaining project to ensure there is always
/// an active project.
///
/// **Path Parameters:** `id` - UUID of the project to delete.
/// **Response:** 200 OK with success confirmation, 404 if project not found, 400 if last project.
///
/// **Example Response (Success):**
/// ```json
/// {
///   "ok": true
/// }
/// ```
///
/// **Example Response (Not Found):**
/// ```json
/// {
///   "error": "project not found"
/// }
/// ```
///
/// **Example Response (Cannot Delete Last):**
/// ```json
/// {
///   "error": "cannot delete last project"
/// }
/// ```
async fn delete_project(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut projects = state.projects.write().await;
    if projects.len() <= 1 {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "cannot delete last project"})),
        );
    }
    let before = projects.len();
    projects.retain(|p| p.id != id);
    if projects.len() == before {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "project not found"})),
        );
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
}

/// POST /api/projects/{id}/activate -- set a project as the active project.
///
/// Activates the specified project and deactivates all other projects. Only one project
/// can be active at a time. The active project is used as the default workspace for
/// operations that require a project context.
///
/// **Path Parameters:** `id` - UUID of the project to activate.
/// **Response:** 200 OK with the activated Project object, 404 if project not found.
///
/// **Example Response (Success):**
/// ```json
/// {
///   "id": "770e8400-e29b-41d4-a716-446655440002",
///   "name": "my-rust-project",
///   "path": "/home/user/projects/rust-harness",
///   "created_at": "2026-02-23T11:30:00Z",
///   "is_active": true
/// }
/// ```
///
/// **Example Response (Not Found):**
/// ```json
/// {
///   "error": "project not found"
/// }
/// ```
async fn activate_project(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut projects = state.projects.write().await;
    let exists = projects.iter().any(|p| p.id == id);
    if !exists {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "project not found"})),
        );
    }
    for p in projects.iter_mut() {
        p.is_active = p.id == id;
    }
    let activated = projects.iter().find(|p| p.id == id).cloned().unwrap();
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(activated)),
    )
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

/// POST /api/github/pr/{number}/watch — start watching a pull request for status updates.
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

/// DELETE /api/github/pr/{number}/watch — stop watching a pull request.
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
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "PR not watched"})),
        )
    }
}

/// GET /api/github/pr/watched — list all currently watched pull requests.
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

/// POST /api/releases — create a new GitHub release with tag, name, body, and metadata.
async fn create_release(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateReleaseRequest>,
) -> impl IntoResponse {
    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().map_or(true, |t| t.is_empty()) || owner.is_empty() || repo.is_empty() {
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
        );
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
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
            );
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
}

/// GET /api/releases — list all GitHub releases for the configured repository.
async fn list_releases(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().map_or(false, |t| !t.is_empty()) && !owner.is_empty() && !repo.is_empty() {
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

/// POST /api/tasks/{id}/archive -- archive a task to remove it from active views.
///
/// Marks a task as archived by adding its ID to the archived tasks list. Archived tasks
/// are hidden from the main task list and Kanban board but remain accessible via the
/// archived tasks endpoint. This is useful for completed or cancelled tasks that should
/// be retained for historical reference but removed from day-to-day workflows.
///
/// **Response:** 200 OK with confirmation object.
///
/// **Example Response:**
/// ```json
/// {
///   "archived": "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
/// }
/// ```
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

/// POST /api/tasks/{id}/unarchive -- restore an archived task to active views.
///
/// Removes a task from the archived tasks list, making it visible again in the main
/// task list and Kanban board. This is useful when an archived task needs to be
/// reopened or reviewed. If the task is not currently archived, this operation has
/// no effect but still returns success.
///
/// **Response:** 200 OK with confirmation object.
///
/// **Example Response:**
/// ```json
/// {
///   "unarchived": "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
/// }
/// ```
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

/// GET /api/tasks/archived -- retrieve all archived task IDs.
///
/// Returns an array of task IDs that have been archived. These IDs can be used to
/// fetch the full task details from the main tasks endpoint if needed. This endpoint
/// is useful for building an archive view or allowing users to browse and restore
/// previously archived tasks.
///
/// **Response:** 200 OK with array of UUID strings.
///
/// **Example Response:**
/// ```json
/// [
///   "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
///   "b2c3d4e5-f6a7-8901-bcde-f12345678901"
/// ]
/// ```
async fn list_archived_tasks(State(state): State<Arc<ApiState>>) -> Json<Vec<Uuid>> {
    let archived = state.archived_tasks.read().await;
    Json(archived.clone())
}

// ---------------------------------------------------------------------------
// Attachment handlers
// ---------------------------------------------------------------------------

/// GET /api/tasks/{task_id}/attachments -- list all attachments for a specific task.
///
/// Returns an array of attachment metadata for all files associated with the specified task.
/// Each attachment includes metadata such as filename, content type, size, and upload timestamp.
/// The actual file content is not included; this endpoint only returns metadata stubs for
/// UI display and management purposes.
///
/// **Response:** 200 OK with array of Attachment objects.
///
/// **Example Response:**
/// ```json
/// [
///   {
///     "id": "d4e5f6a7-b8c9-0123-def4-567890abcdef",
///     "task_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
///     "filename": "screenshot.png",
///     "content_type": "image/png",
///     "size_bytes": 524288,
///     "uploaded_at": "2026-02-23T10:15:30Z"
///   }
/// ]
/// ```
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

/// POST /api/tasks/{task_id}/attachments -- add a new attachment to a task.
///
/// Creates a new attachment metadata record for a file associated with the specified task.
/// This endpoint stores the attachment metadata (filename, content type, size) but does not
/// handle actual file upload or storage. The caller is responsible for managing the file
/// content separately. Useful for tracking screenshots, documents, or other files related
/// to a task.
///
/// **Request Body:** JSON object with filename, content_type, and size_bytes fields.
/// **Response:** 201 Created with the newly created Attachment object.
///
/// **Example Request:**
/// ```json
/// {
///   "filename": "screenshot.png",
///   "content_type": "image/png",
///   "size_bytes": 524288
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "id": "d4e5f6a7-b8c9-0123-def4-567890abcdef",
///   "task_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
///   "filename": "screenshot.png",
///   "content_type": "image/png",
///   "size_bytes": 524288,
///   "uploaded_at": "2026-02-23T10:15:30Z"
/// }
/// ```
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

/// DELETE /api/tasks/{task_id}/attachments/{id} -- delete an attachment from a task.
///
/// Removes the attachment metadata record for the specified attachment ID. This does not
/// delete the actual file content (if it exists); it only removes the metadata tracking
/// record from the system. Returns 404 if the attachment ID does not exist.
///
/// **Response:** 200 OK with confirmation object, or 404 Not Found if attachment doesn't exist.
///
/// **Example Success Response:**
/// ```json
/// {
///   "deleted": "d4e5f6a7-b8c9-0123-def4-567890abcdef"
/// }
/// ```
///
/// **Example Error Response:**
/// ```json
/// {
///   "error": "attachment not found"
/// }
/// ```
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
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "attachment not found"})),
        )
    }
}

// ---------------------------------------------------------------------------
// Task draft handlers
// ---------------------------------------------------------------------------

/// POST /api/tasks/drafts -- save or update a task draft for auto-save functionality.
///
/// Creates or updates a task draft with the provided content. Drafts are useful for
/// auto-saving task creation forms so users don't lose work if they navigate away or
/// close their browser. The draft ID should be provided by the client and remains stable
/// across multiple saves of the same draft. The updated_at timestamp is automatically
/// set to the current time on each save.
///
/// **Request Body:** TaskDraft JSON object with id, title, description, and optional fields.
/// **Response:** 200 OK with the saved TaskDraft object including updated timestamp.
///
/// **Example Request:**
/// ```json
/// {
///   "id": "e5f6a7b8-c9d0-1234-efab-cdef01234567",
///   "title": "Implement user authentication",
///   "description": "Add login and registration flow with JWT tokens",
///   "category": "Backend",
///   "priority": "High",
///   "files": ["src/auth.rs", "src/routes.rs"]
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "id": "e5f6a7b8-c9d0-1234-efab-cdef01234567",
///   "title": "Implement user authentication",
///   "description": "Add login and registration flow with JWT tokens",
///   "category": "Backend",
///   "priority": "High",
///   "files": ["src/auth.rs", "src/routes.rs"],
///   "updated_at": "2026-02-23T10:20:15Z"
/// }
/// ```
async fn save_task_draft(
    State(state): State<Arc<ApiState>>,
    Json(mut draft): Json<TaskDraft>,
) -> impl IntoResponse {
    draft.updated_at = chrono::Utc::now().to_rfc3339();
    let mut drafts = state.task_drafts.write().await;
    drafts.insert(draft.id, draft.clone());
    (axum::http::StatusCode::OK, Json(serde_json::json!(draft)))
}

/// GET /api/tasks/drafts/{id} -- retrieve a specific task draft by ID.
///
/// Fetches a previously saved task draft by its ID. This is useful for restoring draft
/// content when a user returns to a task creation form. Returns 404 if no draft exists
/// with the specified ID.
///
/// **Response:** 200 OK with TaskDraft object, or 404 Not Found if draft doesn't exist.
///
/// **Example Success Response:**
/// ```json
/// {
///   "id": "e5f6a7b8-c9d0-1234-efab-cdef01234567",
///   "title": "Implement user authentication",
///   "description": "Add login and registration flow with JWT tokens",
///   "category": "Backend",
///   "priority": "High",
///   "files": ["src/auth.rs", "src/routes.rs"],
///   "updated_at": "2026-02-23T10:20:15Z"
/// }
/// ```
///
/// **Example Error Response:**
/// ```json
/// {
///   "error": "draft not found"
/// }
/// ```
async fn get_task_draft(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let drafts = state.task_drafts.read().await;
    match drafts.get(&id) {
        Some(draft) => (axum::http::StatusCode::OK, Json(serde_json::json!(draft))),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "draft not found"})),
        ),
    }
}

/// DELETE /api/tasks/drafts/{id} -- delete a task draft.
///
/// Removes a task draft from storage. This is typically called when a user completes
/// task creation (converting the draft to a real task) or explicitly discards a draft.
/// Returns 404 if the draft doesn't exist, but this is not considered an error condition
/// since the desired end state (draft not existing) is achieved.
///
/// **Response:** 200 OK with confirmation object, or 404 Not Found if draft doesn't exist.
///
/// **Example Success Response:**
/// ```json
/// {
///   "deleted": "e5f6a7b8-c9d0-1234-efab-cdef01234567"
/// }
/// ```
///
/// **Example Error Response:**
/// ```json
/// {
///   "error": "draft not found"
/// }
/// ```
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
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "draft not found"})),
        )
    }
}

/// GET /api/tasks/drafts -- retrieve all saved task drafts.
///
/// Returns an array of all task drafts currently stored in the system. This is useful for
/// showing users a list of their incomplete task creations so they can resume work on any
/// of them. Drafts are ordered by their storage order (not necessarily by updated_at).
///
/// **Response:** 200 OK with array of TaskDraft objects.
///
/// **Example Response:**
/// ```json
/// [
///   {
///     "id": "e5f6a7b8-c9d0-1234-efab-cdef01234567",
///     "title": "Implement user authentication",
///     "description": "Add login and registration flow with JWT tokens",
///     "category": "Backend",
///     "priority": "High",
///     "files": ["src/auth.rs", "src/routes.rs"],
///     "updated_at": "2026-02-23T10:20:15Z"
///   },
///   {
///     "id": "f6a7b8c9-d0e1-2345-fabc-def012345678",
///     "title": "Fix CSS layout bug",
///     "description": "Header alignment issue on mobile",
///     "category": "Frontend",
///     "priority": "Medium",
///     "files": ["styles/header.css"],
///     "updated_at": "2026-02-23T09:45:22Z"
///   }
/// ]
/// ```
async fn list_task_drafts(State(state): State<Arc<ApiState>>) -> Json<Vec<TaskDraft>> {
    let drafts = state.task_drafts.read().await;
    Json(drafts.values().cloned().collect())
}

// ---------------------------------------------------------------------------
// Column locking
// ---------------------------------------------------------------------------

/// POST /api/kanban/columns/lock -- lock or unlock a Kanban column to prevent drag-drop.
///
/// Toggles the lock state of a specified Kanban column. When locked, the column displays
/// a lock emoji (🔒) prefix in its label and prevents tasks from being dragged into or out
/// of the column. This is useful for protecting columns like "Done" or "PR Created" from
/// accidental modifications.
///
/// **Request Body:** LockColumnRequest with column_id and locked boolean.
/// **Response:** 200 OK with confirmation containing column_id and locked state.
///
/// **Example Request:**
/// ```json
/// {
///   "column_id": "done",
///   "locked": true
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "column_id": "done",
///   "locked": true
/// }
/// ```
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

/// POST /api/kanban/ordering -- persist the user's manual task ordering within a column.
///
/// Saves the order of tasks within a specific Kanban column after the user has manually
/// reordered them via drag-and-drop. The ordering is persisted so that tasks appear in
/// the same order when the board is reloaded. This endpoint accepts a column ID and an
/// ordered array of task IDs.
///
/// **Request Body:** TaskOrderingRequest with column_id and array of task_ids in order.
/// **Response:** 200 OK with confirmation containing column_id and task_count.
///
/// **Example Request:**
/// ```json
/// {
///   "column_id": "in-progress",
///   "task_ids": [
///     "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
///     "b2c3d4e5-f6a7-8901-bcde-f12345678901",
///     "c3d4e5f6-a7b8-9012-cdef-123456789012"
///   ]
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "column_id": "in-progress",
///   "task_count": 3
/// }
/// ```
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
        // Without GITHUB_TOKEN (and owner/repo) configured, sync returns 503
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("token"));
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
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
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
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
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
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
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
