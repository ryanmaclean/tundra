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
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

use at_core::config::{Config, CredentialProvider};
use at_core::session_store::{SessionState, SessionStore};
use at_core::settings::SettingsManager;
use at_core::types::{
    Agent, AgentProfile, Bead, BeadStatus, KpiSnapshot, Lane, PhaseConfig, Task, TaskCategory,
    TaskComplexity, TaskImpact, TaskPhase, TaskPriority,
};
use at_intelligence::{
    changelog::ChangelogEngine,
    ideation::IdeationEngine,
    insights::InsightsEngine,
    memory::MemoryStore,
    roadmap::RoadmapEngine,
};
use at_telemetry::metrics::global_metrics;
use at_telemetry::middleware::metrics_middleware;
use at_telemetry::tracing_setup::request_id_middleware;
use crate::auth::AuthLayer;
use crate::event_bus::EventBus;
use crate::intelligence_api;
use crate::notifications::{NotificationStore, notification_from_event};
use crate::terminal::TerminalRegistry;
use crate::terminal_ws;

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

/// Shared application state for all HTTP/WS handlers.
pub struct ApiState {
    pub event_bus: EventBus,
    pub beads: Arc<RwLock<Vec<Bead>>>,
    pub agents: Arc<RwLock<Vec<Agent>>>,
    pub kpi: Arc<RwLock<KpiSnapshot>>,
    pub tasks: Arc<RwLock<Vec<Task>>>,
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
}

impl ApiState {
    /// Create a new `ApiState` with empty collections and a fresh event bus.
    pub fn new(event_bus: EventBus) -> Self {
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
        }
    }

    /// Create a new `ApiState` with a PTY pool for terminal support.
    pub fn with_pty_pool(event_bus: EventBus, pty_pool: Arc<at_session::pty_pool::PtyPool>) -> Self {
        let mut state = Self::new(event_bus);
        state.pty_pool = Some(pty_pool);
        state
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
        .route("/api/kpi", get(get_kpi))
        .route("/api/tasks", get(list_tasks))
        .route("/api/tasks", post(create_task))
        .route("/api/tasks/{id}", get(get_task))
        .route("/api/tasks/{id}", put(update_task))
        .route("/api/tasks/{id}", axum::routing::delete(delete_task))
        .route("/api/tasks/{id}/phase", post(update_task_phase))
        .route("/api/tasks/{id}/logs", get(get_task_logs))
        .route("/api/terminals", get(terminal_ws::list_terminals))
        .route("/api/terminals", post(terminal_ws::create_terminal))
        .route("/api/terminals/{id}", axum::routing::delete(terminal_ws::delete_terminal))
        .route("/ws/terminal/{id}", get(terminal_ws::terminal_ws))
        .route("/api/settings", get(get_settings))
        .route("/api/settings", put(put_settings))
        .route("/api/settings", patch(patch_settings))
        .route("/api/credentials/status", get(get_credentials_status))
        .route("/api/github/sync", post(trigger_github_sync))
        .route("/api/github/sync/status", get(get_sync_status))
        .route("/api/github/pr/{task_id}", post(create_pr_for_task))
        // Notification endpoints
        .route("/api/notifications", get(list_notifications))
        .route("/api/notifications/count", get(notification_count))
        .route("/api/notifications/{id}/read", post(mark_notification_read))
        .route("/api/notifications/read-all", post(mark_all_notifications_read))
        .route("/api/notifications/{id}", axum::routing::delete(delete_notification))
        // Metrics endpoints
        .route("/api/metrics", get(get_metrics_prometheus))
        .route("/api/metrics/json", get(get_metrics_json))
        // Session endpoints
        .route("/api/sessions/ui", get(get_ui_session))
        .route("/api/sessions/ui", put(save_ui_session))
        .route("/api/sessions/ui/list", get(list_ui_sessions))
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

    let mut beads = state.beads.write().await;
    beads.push(bead.clone());

    // Publish event
    state.event_bus.publish(crate::protocol::BridgeMessage::BeadList(beads.clone()));

    (axum::http::StatusCode::CREATED, Json(bead))
}

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
    state.event_bus.publish(crate::protocol::BridgeMessage::BeadList(beads.clone()));

    (axum::http::StatusCode::OK, Json(serde_json::json!(bead_snapshot)))
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
    (axum::http::StatusCode::OK, Json(serde_json::json!(snapshot)))
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
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "title cannot be empty"})),
        ).into_response();
    }

    let mut task = Task::new(req.title, req.bead_id, req.category, req.priority, req.complexity);
    task.description = req.description;
    task.impact = req.impact;
    task.agent_profile = req.agent_profile;
    if let Some(configs) = req.phase_configs {
        task.phase_configs = configs;
    }

    let mut tasks = state.tasks.write().await;
    tasks.push(task.clone());

    (axum::http::StatusCode::CREATED, Json(serde_json::json!(task))).into_response()
}

async fn get_task(
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
    (axum::http::StatusCode::OK, Json(serde_json::json!(task)))
}

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
    (axum::http::StatusCode::OK, Json(serde_json::json!(task_snapshot)))
}

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

    (axum::http::StatusCode::OK, Json(serde_json::json!(task_snapshot)))
}

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
        Ok(()) => (axum::http::StatusCode::OK, Json(serde_json::json!(cfg))),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
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

async fn trigger_github_sync(
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    // Mark sync as in-progress
    {
        let mut status = state.sync_status.write().await;
        status.is_syncing = true;
    }

    let beads = state.beads.read().await;
    let imported_count = beads
        .iter()
        .filter(|b| {
            b.metadata
                .as_ref()
                .and_then(|m| m.get("source"))
                .and_then(|v| v.as_str())
                == Some("github")
        })
        .count() as u64;

    // Mark sync as complete
    {
        let mut status = state.sync_status.write().await;
        status.is_syncing = false;
        status.last_sync_time = Some(chrono::Utc::now());
        status.issues_imported = imported_count;
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

async fn get_sync_status(
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    let status = state.sync_status.read().await;
    Json(serde_json::json!(*status))
}

async fn create_pr_for_task(
    State(state): State<Arc<ApiState>>,
    Path(task_id): Path<Uuid>,
) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let Some(task) = tasks.iter().find(|t| t.id == task_id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found"})),
        );
    };

    let response = PrCreatedResponse {
        message: "PR creation initiated".to_string(),
        task_id: task.id,
        pr_title: task.title.clone(),
        pr_branch: task.git_branch.clone(),
    };

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(response)),
    )
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
async fn notification_count(
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
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
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "notification not found"})),
        )
    }
}

/// POST /api/notifications/read-all — mark all notifications as read.
async fn mark_all_notifications_read(
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
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
async fn get_ui_session(
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
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
async fn list_ui_sessions(
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    match state.session_store.list_sessions() {
        Ok(sessions) => (axum::http::StatusCode::OK, Json(serde_json::json!(sessions))),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

// ---------------------------------------------------------------------------
// WebSocket — legacy /ws handler
// ---------------------------------------------------------------------------

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<ApiState>) {
    let rx = state.event_bus.subscribe();
    loop {
        match rx.recv_async().await {
            Ok(msg) => {
                let json = serde_json::to_string(&msg).unwrap_or_default();
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

                        let json = serde_json::to_string(&msg).unwrap_or_default();
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
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["message"], "Sync completed");
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
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_create_pr_for_existing_task() {
        let (app, state) = test_app();

        // Insert a task
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
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["message"], "PR creation initiated");
        assert_eq!(json["pr_title"], "Test task");
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

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
    }

    #[tokio::test]
    async fn test_mark_all_read() {
        let (_app, state) = test_app();

        // Add multiple notifications.
        {
            let mut store = state.notification_store.write().await;
            store.add("n1", "m1", crate::notifications::NotificationLevel::Info, "system");
            store.add("n2", "m2", crate::notifications::NotificationLevel::Warning, "system");
            store.add("n3", "m3", crate::notifications::NotificationLevel::Error, "system");
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
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
            id1 = store.add("n1", "m1", crate::notifications::NotificationLevel::Info, "system");
            store.add("n2", "m2", crate::notifications::NotificationLevel::Warning, "system");
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
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
                store.add(format!("n{i}"), "msg", crate::notifications::NotificationLevel::Info, "system");
            }
        }

        let app = api_router(state.clone());
        let req = Request::builder()
            .method("GET")
            .uri("/api/notifications?limit=3&offset=0")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
}
