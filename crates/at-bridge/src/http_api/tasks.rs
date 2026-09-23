use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;

use at_core::merge_gate::validate_criteria;
use at_core::types::{Task, TaskPhase, TaskSource};

use super::state::ApiState;
use super::types::{CreateTaskRequest, TaskListQuery, UpdateTaskPhaseRequest, UpdateTaskRequest};
use super::validate_text_field;
use crate::api_error::ApiError;

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
pub(crate) async fn list_tasks(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<TaskListQuery>,
) -> Json<Vec<Task>> {
    let tasks = state.tasks.read().await;
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);

    let filtered: Vec<Task> = tasks
        .values()
        .filter(|task| {
            if let Some(bead_id) = query.bead_id {
                if task.bead_id != bead_id {
                    return false;
                }
            }

            // Filter by phase if specified
            if let Some(ref phase_str) = query.phase {
                let task_phase_str = serde_json::to_string(&task.phase)
                    .unwrap_or_default()
                    .trim_matches('"')
                    .to_lowercase();
                if !task_phase_str.eq_ignore_ascii_case(phase_str) {
                    return false;
                }
            }

            // Filter by category if specified
            if let Some(ref category_str) = query.category {
                let task_category_str = serde_json::to_string(&task.category)
                    .unwrap_or_default()
                    .trim_matches('"')
                    .to_lowercase();
                if !task_category_str.eq_ignore_ascii_case(category_str) {
                    return false;
                }
            }

            // Filter by priority if specified
            if let Some(ref priority_str) = query.priority {
                let task_priority_str = serde_json::to_string(&task.priority)
                    .unwrap_or_default()
                    .trim_matches('"')
                    .to_lowercase();
                if !task_priority_str.eq_ignore_ascii_case(priority_str) {
                    return false;
                }
            }

            // Filter by source if specified
            if let Some(ref source_str) = query.source {
                if let Some(ref task_source) = task.source {
                    let task_source_str = match task_source {
                        TaskSource::Manual => "manual".to_string(),
                        TaskSource::GithubIssue { .. } => "github_issue".to_string(),
                        TaskSource::GithubPr { .. } => "github_pr".to_string(),
                        TaskSource::GitlabIssue { .. } => "gitlab_issue".to_string(),
                        TaskSource::LinearIssue { .. } => "linear_issue".to_string(),
                        TaskSource::Import => "import".to_string(),
                        TaskSource::Ideation { .. } => "ideation".to_string(),
                    };
                    if !task_source_str.eq_ignore_ascii_case(source_str) {
                        return false;
                    }
                } else {
                    // Task has no source, but filter requires one
                    return false;
                }
            }

            true
        })
        .skip(offset)
        .take(limit)
        .cloned()
        .collect();

    Json(filtered)
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
///   "bead_id": "550e8400-e29b-41d4-a716-446655440000",
///   "category": "Backend",
///   "priority": "High",
///   "complexity": "Medium"
/// }
/// ```
pub(crate) async fn create_task(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let task = create_task_checked(&state, req).await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(task)),
    )
        .into_response())
}

/// Shared by `POST /api/tasks` and the MCP `create_task` tool: validates
/// `title`/`description`, resolves `acceptance_criteria` (explicit value, or
/// inherited from the bead's `metadata.acceptance_criteria` -- never from
/// issue bodies or spec prose), validates them, inserts the task and returns
/// it. Does not publish a `TaskUpdate` (there is nothing to update yet).
pub(crate) async fn create_task_checked(
    state: &ApiState,
    req: CreateTaskRequest,
) -> Result<Task, ApiError> {
    validate_text_field(&req.title).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    if let Some(ref description) = req.description {
        validate_text_field(description).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    }

    // Explicit criteria win; otherwise inherit the bead's (set via
    // POST /api/beads or the MCP create_bead tool). Criteria are never
    // derived from issue bodies or spec prose.
    let acceptance_criteria = match req.acceptance_criteria {
        Some(criteria) => criteria,
        None => bead_acceptance_criteria(state, req.bead_id).await,
    };
    validate_criteria(&acceptance_criteria).map_err(ApiError::BadRequest)?;

    let mut task = Task::new(
        req.title,
        req.bead_id,
        req.category,
        req.priority,
        req.complexity,
    );
    task.acceptance_criteria = acceptance_criteria;
    task.description = req.description;
    task.impact = req.impact;
    task.agent_profile = req.agent_profile;
    task.source = req.source;
    if let Some(configs) = req.phase_configs {
        task.phase_configs = configs;
    }

    state.tasks.write().await.insert(task.id, task.clone());
    Ok(task)
}

/// Criteria stored on a bead as `metadata.acceptance_criteria` (empty when
/// the bead is unknown or has none).
async fn bead_acceptance_criteria(state: &ApiState, bead_id: Uuid) -> Vec<String> {
    let beads = state.beads.read().await;
    beads
        .get(&bead_id)
        .and_then(|b| b.metadata.as_ref())
        .and_then(|m| m.get("acceptance_criteria"))
        .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
        .unwrap_or_default()
}

/// `true` while the execute pipeline may be reading a task's acceptance
/// criteria; changes are refused so the gate never runs a moving target.
pub(crate) fn criteria_locked(phase: &TaskPhase) -> bool {
    matches!(
        phase,
        TaskPhase::Coding | TaskPhase::Qa | TaskPhase::Fixing | TaskPhase::Merging
    )
}

/// GET /api/tasks/{id} -- retrieve a specific task by ID.
///
/// Returns the complete task object including all metadata, phase information,
/// timestamps, and agent assignment details.
///
/// **Path Parameters:** `id` - UUID of the task to retrieve.
/// **Response:** 200 OK with Task object, 404 if not found.
pub(crate) async fn get_task(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let tasks = state.tasks.read().await;
    let Some(task) = tasks.get(&id) else {
        return Err(ApiError::NotFound("task not found".into()));
    };
    Ok((axum::http::StatusCode::OK, Json(serde_json::json!(task))))
}

/// PUT /api/tasks/{id} -- update an existing task.
///
/// Updates one or more fields of an existing task. All fields are optional; only provided
/// fields will be updated. Updates the task's `updated_at` timestamp and broadcasts a
/// TaskUpdate event via the event bus for real-time UI updates.
///
/// **Path Parameters:** `id` - UUID of the task to update.
/// **Request Body:** UpdateTaskRequest JSON object with optional fields.
/// **Response:** 200 OK with updated Task, 404 if not found, 400 if validation fails.
///
/// `acceptance_criteria`: omitted = unchanged, `[]` = clear, list = replace.
/// While the task is in Coding, Qa, Fixing or Merging a criteria change is
/// refused with 409 `{"error": "acceptance_criteria_locked", "phase"}` and
/// nothing else in the request is applied.
pub(crate) async fn update_task(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTaskRequest>,
) -> Result<axum::response::Response, ApiError> {
    if let Some(ref criteria) = req.acceptance_criteria {
        validate_criteria(criteria).map_err(ApiError::BadRequest)?;
    }
    let mut tasks = state.tasks.write().await;
    let Some(task) = tasks.get_mut(&id) else {
        return Err(ApiError::NotFound("task not found".into()));
    };
    if req.acceptance_criteria.is_some() && criteria_locked(&task.phase) {
        return Ok((
            axum::http::StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "acceptance_criteria_locked",
                "phase": task.phase,
                "message": "acceptance criteria cannot change while the pipeline runs",
            })),
        )
            .into_response());
    }

    if let Some(title) = req.title {
        if title.is_empty() {
            return Err(ApiError::BadRequest("title cannot be empty".into()));
        }
        // Validate title
        if let Err(e) = validate_text_field(&title) {
            return Err(ApiError::BadRequest(e.to_string()));
        }
        task.title = title;
    }
    if let Some(desc) = req.description {
        // Validate description
        if let Err(e) = validate_text_field(&desc) {
            return Err(ApiError::BadRequest(e.to_string()));
        }
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
    if let Some(criteria) = req.acceptance_criteria {
        task.acceptance_criteria = criteria;
    }
    task.updated_at = chrono::Utc::now();

    let task_snapshot = task.clone();
    drop(tasks);
    let response_json = serde_json::json!(task_snapshot);
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(
            task_snapshot,
        )));
    Ok((axum::http::StatusCode::OK, Json(response_json)).into_response())
}

/// DELETE /api/tasks/{id} -- delete a task.
///
/// Permanently removes a task from the system. This operation cannot be undone.
///
/// **Path Parameters:** `id` - UUID of the task to delete.
/// **Response:** 200 OK with deletion confirmation, 404 if not found.
pub(crate) async fn delete_task(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let mut tasks = state.tasks.write().await;
    if tasks.remove(&id).is_none() {
        return Err(ApiError::NotFound("task not found".into()));
    }
    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::json!({"status": "deleted", "id": id.to_string()})),
    ))
}

/// POST /api/tasks/{id}/phase -- update a task's phase/stage.
///
/// Transitions a task to a new phase (Pending, Planning, Coding, QA, etc.) with
/// validation to ensure the transition is valid according to the task lifecycle.
/// Publishes a TaskUpdate event for real-time WebSocket notifications.
///
/// **Request Body:** UpdateTaskPhaseRequest JSON object with target phase.
/// **Response:** 200 OK with updated Task object, 404 if task not found, 400 if invalid transition.
pub(crate) async fn update_task_phase(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTaskPhaseRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mut tasks = state.tasks.write().await;
    let Some(task) = tasks.get_mut(&id) else {
        return Err(ApiError::NotFound("task not found".into()));
    };

    if !task.phase.can_transition_to(&req.phase) {
        return Err(ApiError::BadRequest(format!(
            "invalid phase transition from {:?} to {:?}",
            task.phase, req.phase
        )));
    }

    task.set_phase(req.phase);
    let task_snapshot = task.clone();
    drop(tasks);
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(
            task_snapshot.clone(),
        )));
    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::json!(task_snapshot)),
    ))
}

/// GET /api/tasks/{id}/logs -- retrieve execution logs for a task.
///
/// Returns the accumulated log output from task execution phases (Planning, Coding, QA).
///
/// **Response:** 200 OK with array of log strings, 404 if task not found.
pub(crate) async fn get_task_logs(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let tasks = state.tasks.read().await;
    let Some(task) = tasks.get(&id) else {
        return Err(ApiError::NotFound("task not found".into()));
    };
    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::json!(task.logs)),
    ))
}
