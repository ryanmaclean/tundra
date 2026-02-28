use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;

use at_core::types::{Bead, Lane};

use super::state::ApiState;
use super::types::{BeadQuery, CreateBeadRequest, UpdateBeadStatusRequest};
use super::validate_text_field;

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
        beads
            .values()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
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
) -> impl IntoResponse {
    // Validate title
    if let Err(e) = validate_text_field(&req.title) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }

    // Validate description if present
    if let Some(ref description) = req.description {
        if let Err(e) = validate_text_field(description) {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response();
        }
    }

    let lane = req.lane.unwrap_or(Lane::Standard);
    let mut bead = Bead::new(req.title, lane);
    bead.description = req.description;
    if let Some(tags) = req.tags {
        bead.metadata = Some(serde_json::json!({ "tags": tags }));
    }

    let mut beads = state.beads.write().await;
    beads.insert(bead.id, bead.clone());

    // Publish event
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::BeadCreated(bead.clone()));

    (axum::http::StatusCode::CREATED, Json(bead)).into_response()
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
) -> impl IntoResponse {
    let mut beads = state.beads.write().await;
    let Some(bead) = beads.get_mut(&id) else {
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
        .publish(crate::protocol::BridgeMessage::BeadUpdated(bead_snapshot.clone()));

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(bead_snapshot)),
    )
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
) -> impl IntoResponse {
    let mut beads = state.beads.write().await;
    if beads.remove(&id).is_none() {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "bead not found"})),
        );
    }

    // Publish updated bead list event
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::BeadList(beads.values().cloned().collect()));

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({"status": "deleted", "id": id.to_string()})),
    )
}
