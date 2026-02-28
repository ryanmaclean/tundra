use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;

use at_core::types::Agent;

use super::state::ApiState;
use super::types::AgentQuery;
use crate::api_error::ApiError;

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
pub(crate) async fn list_agents(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<AgentQuery>,
) -> Json<Vec<Agent>> {
    let agents = state.agents.read().await;
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);
    Json(agents.values().skip(offset).take(limit).cloned().collect())
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
/// ```text
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
pub(crate) async fn nudge_agent(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let mut agents = state.agents.write().await;
    let Some(agent) = agents.get_mut(&id) else {
        return Err(ApiError::NotFound("agent not found".into()));
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
    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::json!(snapshot)),
    ))
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
/// ```text
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
pub(crate) async fn stop_agent(State(state): State<Arc<ApiState>>, Path(id): Path<Uuid>) -> Result<impl IntoResponse, ApiError> {
    let mut agents = state.agents.write().await;
    let Some(agent) = agents.get_mut(&id) else {
        return Err(ApiError::NotFound("agent not found".into()));
    };

    agent.status = at_core::types::AgentStatus::Stopped;
    agent.last_seen = chrono::Utc::now();

    let snapshot = agent.clone();
    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::json!(snapshot)),
    ))
}
