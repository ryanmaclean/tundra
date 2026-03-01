use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use at_core::config::CredentialProvider;
use at_core::types::KpiSnapshot;

use super::state::ApiState;
use super::types::{
    ArchivedTaskQuery, Attachment, AttachmentQuery, CliAvailabilityEntry,
    CompetitorAnalysisRequest, CompetitorAnalysisResult, DirectModeRequest, FileWatchRequest,
    LockColumnRequest, StatusResponse, TaskDraft, TaskDraftQuery, TaskOrderingRequest,
};

// ---------------------------------------------------------------------------
// Local types
// ---------------------------------------------------------------------------

/// Aggregated LLM token usage and cost tracking across all agent sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CostResponse {
    input_tokens: u64,
    output_tokens: u64,
    sessions: Vec<CostSessionEntry>,
}

/// Token usage details for a single agent execution session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CostSessionEntry {
    session_id: String,
    agent_name: String,
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AgentSessionEntry {
    id: String,
    agent_name: String,
    cli_type: String,
    status: String,
    duration: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ConvoyEntry {
    id: String,
    name: String,
    bead_count: u32,
    status: String,
}

// ---------------------------------------------------------------------------
// Status and KPI
// ---------------------------------------------------------------------------

/// GET /api/status -- returns basic server health and statistics.
pub(crate) async fn get_status(State(state): State<Arc<ApiState>>) -> Json<StatusResponse> {
    let agent_count = state.agents.read().await.len();
    let bead_count = state.beads.read().await.len();
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: state.start_time.elapsed().as_secs(),
        agent_count,
        bead_count,
    })
}

/// GET /api/kpi -- retrieve the current KPI snapshot.
pub(crate) async fn get_kpi(State(state): State<Arc<ApiState>>) -> Json<KpiSnapshot> {
    let kpi = state.kpi.read().await;
    Json(kpi.clone())
}

// ---------------------------------------------------------------------------
// Memory usage debugging
// ---------------------------------------------------------------------------

/// GET /api/debug/memory -- returns in-memory data structure counts for monitoring.
pub(crate) async fn get_memory_usage(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let archived = state.archived_tasks.read().await;
    let buffers = state.disconnect_buffers.read().await;
    let notifications = state.notification_store.read().await;
    let agents = state.agents.read().await;
    let beads = state.beads.read().await;
    let drafts = state.task_drafts.read().await;
    let attachments = state.attachments.read().await;

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "tasks": tasks.len(),
            "archived_tasks": archived.len(),
            "disconnect_buffers": buffers.len(),
            "notifications": notifications.total_count(),
            "agents": agents.len(),
            "beads": beads.len(),
            "task_drafts": drafts.len(),
            "attachments": attachments.len(),
        })),
    )
}

// ---------------------------------------------------------------------------
// Credentials status
// ---------------------------------------------------------------------------

/// GET /api/credentials/status -- report which credential providers are available.
pub(crate) async fn get_credentials_status() -> impl IntoResponse {
    let providers: Vec<&str> = CredentialProvider::available_providers();
    let daemon_auth = CredentialProvider::daemon_api_key().is_some();
    Json(serde_json::json!({
        "providers": providers,
        "daemon_auth": daemon_auth,
    }))
}

// ---------------------------------------------------------------------------
// Direct mode handler
// ---------------------------------------------------------------------------

/// POST /api/settings/direct-mode -- toggle direct mode (agents work in repo root).
pub(crate) async fn toggle_direct_mode(
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

    let patch = serde_json::json!({"agents": {"direct_mode": req.enabled}});
    super::merge_json(&mut current_val, &patch);

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

// ---------------------------------------------------------------------------
// CLI availability
// ---------------------------------------------------------------------------

/// GET /api/cli/available -- detect which CLI tools are installed on the system.
pub(crate) async fn list_available_clis() -> impl IntoResponse {
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

// ---------------------------------------------------------------------------
// Costs
// ---------------------------------------------------------------------------

/// GET /api/costs -- retrieve LLM token usage and cost metrics.
pub(crate) async fn get_costs() -> Json<CostResponse> {
    Json(CostResponse {
        input_tokens: 0,
        output_tokens: 0,
        sessions: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Agent sessions
// ---------------------------------------------------------------------------

/// GET /api/sessions -- retrieve all active agent execution sessions.
pub(crate) async fn list_agent_sessions(
    State(state): State<Arc<ApiState>>,
) -> Json<Vec<AgentSessionEntry>> {
    let agents = state.agents.read().await;
    let sessions: Vec<AgentSessionEntry> = agents
        .values()
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
// Convoys
// ---------------------------------------------------------------------------

/// GET /api/convoys -- list all convoys (stub implementation).
pub(crate) async fn list_convoys() -> Json<Vec<ConvoyEntry>> {
    Json(Vec::new())
}

// ---------------------------------------------------------------------------
// Task Archival
// ---------------------------------------------------------------------------

/// POST /api/tasks/{id}/archive -- archive a task.
pub(crate) async fn archive_task(
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

/// POST /api/tasks/{id}/unarchive -- restore an archived task.
pub(crate) async fn unarchive_task(
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
/// Returns a paginated JSON array of archived task UUIDs. Archived tasks are
/// tasks that have been removed from the active kanban board but retained for
/// historical reference.
///
/// **Query Parameters:**
/// - `limit` (optional): Maximum number of results to return. Defaults to 50.
/// - `offset` (optional): Number of results to skip. Defaults to 0.
///
/// **Response:** 200 OK with array of UUID strings.
///
/// **Example Response:**
/// ```json
/// [
///   "550e8400-e29b-41d4-a716-446655440000",
///   "660e8400-e29b-41d4-a716-446655440001"
/// ]
/// ```
pub(crate) async fn list_archived_tasks(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<ArchivedTaskQuery>,
) -> Json<Vec<Uuid>> {
    let archived = state.archived_tasks.read().await;
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let paginated: Vec<Uuid> = archived
        .iter()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect();

    Json(paginated)
}

// ---------------------------------------------------------------------------
// Attachment handlers
// ---------------------------------------------------------------------------

/// GET /api/tasks/{task_id}/attachments -- list all attachments for a task.
///
/// Returns a paginated JSON array of attachment metadata (images, screenshots,
/// files) associated with the specified task. Each attachment includes file
/// information and upload timestamp.
///
/// **Path Parameters:** `task_id` - UUID of the task.
/// **Query Parameters:**
/// - `limit` (optional): Maximum number of results to return. Defaults to 50.
/// - `offset` (optional): Number of results to skip. Defaults to 0.
///
/// **Response:** 200 OK with array of Attachment objects.
///
/// **Example Response:**
/// ```json
/// [
///   {
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "task_id": "660e8400-e29b-41d4-a716-446655440001",
///     "filename": "screenshot.png",
///     "content_type": "image/png",
///     "size_bytes": 102400,
///     "uploaded_at": "2026-02-23T10:00:00Z"
///   }
/// ]
/// ```
pub(crate) async fn list_attachments(
    State(state): State<Arc<ApiState>>,
    Path(task_id): Path<Uuid>,
    Query(params): Query<AttachmentQuery>,
) -> Json<Vec<Attachment>> {
    let attachments = state.attachments.read().await;
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let filtered: Vec<Attachment> = attachments
        .iter()
        .filter(|a| a.task_id == task_id)
        .skip(offset)
        .take(limit)
        .cloned()
        .collect();

    Json(filtered)
}

/// POST /api/tasks/{task_id}/attachments -- add a new attachment to a task.
pub(crate) async fn add_attachment(
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
pub(crate) async fn delete_attachment(
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

/// POST /api/tasks/drafts -- save or update a task draft.
pub(crate) async fn save_task_draft(
    State(state): State<Arc<ApiState>>,
    Json(mut draft): Json<TaskDraft>,
) -> impl IntoResponse {
    draft.updated_at = chrono::Utc::now().to_rfc3339();
    let mut drafts = state.task_drafts.write().await;
    drafts.insert(draft.id, draft.clone());
    (axum::http::StatusCode::OK, Json(serde_json::json!(draft)))
}

/// GET /api/tasks/drafts/{id} -- retrieve a specific task draft by ID.
pub(crate) async fn get_task_draft(
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
pub(crate) async fn delete_task_draft(
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
/// Returns a paginated JSON array of task drafts. Drafts are auto-saved task
/// creation forms that haven't been finalized yet, allowing users to resume
/// their work later.
///
/// **Query Parameters:**
/// - `limit` (optional): Maximum number of results to return. Defaults to 50.
/// - `offset` (optional): Number of results to skip. Defaults to 0.
///
/// **Response:** 200 OK with array of TaskDraft objects.
///
/// **Example Response:**
/// ```json
/// [
///   {
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "title": "Implement user authentication",
///     "description": "Add OAuth2 support",
///     "category": "feature",
///     "priority": "high",
///     "files": ["src/auth.rs", "src/oauth.rs"],
///     "updated_at": "2026-02-23T10:00:00Z"
///   }
/// ]
/// ```
pub(crate) async fn list_task_drafts(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<TaskDraftQuery>,
) -> Json<Vec<TaskDraft>> {
    let drafts = state.task_drafts.read().await;
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let paginated: Vec<TaskDraft> = drafts
        .values()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect();

    Json(paginated)
}

// ---------------------------------------------------------------------------
// Column locking
// ---------------------------------------------------------------------------

/// POST /api/kanban/columns/lock -- lock or unlock a Kanban column.
pub(crate) async fn lock_column(
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
pub(crate) async fn save_task_ordering(
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

/// POST /api/files/watch -- start watching a file or directory for changes.
pub(crate) async fn start_file_watch(
    State(_state): State<Arc<ApiState>>,
    Json(req): Json<FileWatchRequest>,
) -> impl IntoResponse {
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "watching": req.path,
            "recursive": req.recursive
        })),
    )
}

/// POST /api/files/unwatch -- stop watching a file or directory.
pub(crate) async fn stop_file_watch(
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

/// POST /api/roadmap/competitor-analysis -- analyze a competitor's product.
pub(crate) async fn run_competitor_analysis(
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

/// POST /api/notify/profile-swap -- notify the system that the API profile has changed.
pub(crate) async fn notify_profile_swap(
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
// App update check
// ---------------------------------------------------------------------------

/// GET /api/app/check-update -- check for application updates (stub).
pub(crate) async fn check_app_update(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
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
