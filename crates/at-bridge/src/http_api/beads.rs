use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;

use at_core::types::{Bead, BeadStatus, Lane};

use super::state::ApiState;
use super::types::{BeadQuery, CreateBeadRequest, UpdateBeadStatusRequest};
use super::validate_text_field;
use crate::api_error::ApiError;

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
pub(crate) async fn list_beads(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<BeadQuery>,
) -> Json<Vec<Bead>> {
    let beads = state.beads.read().await;
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let filtered: Vec<Bead> = if let Some(status) = params.status {
        beads
            .values()
            .filter(|b| b.status == status)
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
    } else {
        beads.values().skip(offset).take(limit).cloned().collect()
    };

    Json(filtered)
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
pub(crate) async fn create_bead(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateBeadRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let bead = create_bead_checked(
        &state,
        req.title,
        req.description,
        req.lane.unwrap_or(Lane::Standard),
        req.tags,
        req.acceptance_criteria,
    )
    .await?;

    Ok((axum::http::StatusCode::CREATED, Json(bead)).into_response())
}

/// POST /api/beads/{id}/status -- update a bead's status.
///
/// Transitions a bead to a new status if the transition is valid according to
/// the bead lifecycle (Pending -> InProgress -> Done, etc.). Updates the bead's
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
///   "status": "InProgress",
///   "updated_at": "2026-02-23T10:30:00Z",
///   "hooked_at": "2026-02-23T10:30:00Z"
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
pub(crate) async fn update_bead_status(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateBeadStatusRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let bead_snapshot = transition_bead_status(&state, id, req.status).await?;

    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::json!(bead_snapshot)),
    ))
}

// ---------------------------------------------------------------------------
// Shared bead services (REST handlers and MCP tools both go through these)
// ---------------------------------------------------------------------------

/// Validate, insert and announce a new bead.
///
/// Runs the same input sanitization as every other text field, inserts the
/// bead and publishes [`BridgeMessage::BeadCreated`](crate::protocol::BridgeMessage)
/// so WebSocket clients and the notification recorder see it.
///
/// `acceptance_criteria` (validated with
/// [`at_core::merge_gate::validate_criteria`]) is stored as
/// `metadata.acceptance_criteria` next to `metadata.tags`; tasks created for
/// the bead without their own criteria inherit it.
pub(crate) async fn create_bead_checked(
    state: &ApiState,
    title: String,
    description: Option<String>,
    lane: Lane,
    tags: Option<Vec<String>>,
    acceptance_criteria: Option<Vec<String>>,
) -> Result<Bead, ApiError> {
    validate_text_field(&title).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    if let Some(ref description) = description {
        validate_text_field(description).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    }
    if let Some(ref criteria) = acceptance_criteria {
        at_core::merge_gate::validate_criteria(criteria).map_err(ApiError::BadRequest)?;
    }

    let mut bead = Bead::new(title, lane);
    bead.description = description;
    let mut metadata = serde_json::Map::new();
    if let Some(tags) = tags {
        metadata.insert("tags".into(), serde_json::json!(tags));
    }
    if let Some(criteria) = acceptance_criteria.filter(|c| !c.is_empty()) {
        metadata.insert("acceptance_criteria".into(), serde_json::json!(criteria));
    }
    if !metadata.is_empty() {
        bead.metadata = Some(serde_json::Value::Object(metadata));
    }

    state.beads.write().await.insert(bead.id, bead.clone());
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::BeadCreated(bead.clone()));
    Ok(bead)
}

/// Move a bead to `status` if the lifecycle allows it, then announce it.
///
/// Returns `NotFound` for an unknown id and `BadRequest` for a transition
/// rejected by [`BeadStatus::can_transition_to`]. On success publishes
/// [`BridgeMessage::BeadUpdated`](crate::protocol::BridgeMessage).
pub(crate) async fn transition_bead_status(
    state: &ApiState,
    id: Uuid,
    status: BeadStatus,
) -> Result<Bead, ApiError> {
    let snapshot = {
        let mut beads = state.beads.write().await;
        let Some(bead) = beads.get_mut(&id) else {
            return Err(ApiError::NotFound("bead not found".into()));
        };
        if !bead.status.can_transition_to(&status) {
            return Err(ApiError::BadRequest(format!(
                "invalid transition from {:?} to {:?}",
                bead.status, status
            )));
        }
        bead.status = status;
        bead.updated_at = chrono::Utc::now();
        bead.clone()
    };
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::BeadUpdated(
            snapshot.clone(),
        ));
    Ok(snapshot)
}

/// DELETE /api/beads/{id} -- delete a bead by ID.
///
/// Removes a bead from the system and publishes an updated bead list event
/// to notify connected WebSocket clients of the change.
///
/// **Path Parameters:** `id` - UUID of the bead to delete.
/// **Response:** 200 OK if deleted, 404 if not found.
///
/// **Example Response (Success):**
/// ```json
/// {
///   "status": "deleted",
///   "id": "550e8400-e29b-41d4-a716-446655440000"
/// }
/// ```
///
/// **Example Response (Error - Not Found):**
/// ```json
/// {
///   "error": "bead not found"
/// }
/// ```
pub(crate) async fn delete_bead(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let mut beads = state.beads.write().await;
    if beads.remove(&id).is_none() {
        return Err(ApiError::NotFound("bead not found".into()));
    }

    // Publish granular delete event so clients can remove the single entry
    // without receiving the entire collection.
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::BeadDeleted(id));

    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::json!({"status": "deleted", "id": id.to_string()})),
    ))
}
