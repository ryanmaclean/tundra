use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use at_core::types::{
    Agent, AgentRole, AgentStatus, Bead, BeadStatus, CliType, Lane,
};

use crate::error::TauriError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub agents_active: u64,
    pub beads_total: u64,
    pub beads_active: u64,
    pub cost_today_usd: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiResponse {
    pub total_beads: u64,
    pub backlog: u64,
    pub hooked: u64,
    pub slung: u64,
    pub review: u64,
    pub done: u64,
    pub failed: u64,
    pub escalated: u64,
    pub active_agents: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeadResponse {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub status: BeadStatus,
    pub lane: Lane,
    pub priority: i32,
    pub agent_id: Option<Uuid>,
    pub convoy_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub hooked_at: Option<DateTime<Utc>>,
    pub slung_at: Option<DateTime<Utc>>,
    pub done_at: Option<DateTime<Utc>>,
    pub git_branch: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub id: Uuid,
    pub name: String,
    pub role: AgentRole,
    pub cli_type: CliType,
    pub model: Option<String>,
    pub status: AgentStatus,
    pub rig: Option<String>,
    pub pid: Option<u32>,
    pub session_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn bead_to_response(b: &Bead) -> BeadResponse {
    BeadResponse {
        id: b.id,
        title: b.title.clone(),
        description: b.description.clone(),
        status: b.status.clone(),
        lane: b.lane.clone(),
        priority: b.priority,
        agent_id: b.agent_id,
        convoy_id: b.convoy_id,
        created_at: b.created_at,
        updated_at: b.updated_at,
        hooked_at: b.hooked_at,
        slung_at: b.slung_at,
        done_at: b.done_at,
        git_branch: b.git_branch.clone(),
        metadata: b.metadata.clone(),
    }
}

fn agent_to_response(a: &Agent) -> AgentResponse {
    AgentResponse {
        id: a.id,
        name: a.name.clone(),
        role: a.role.clone(),
        cli_type: a.cli_type.clone(),
        model: a.model.clone(),
        status: a.status.clone(),
        rig: a.rig.clone(),
        pid: a.pid,
        session_id: a.session_id.clone(),
        created_at: a.created_at,
        last_seen: a.last_seen,
        metadata: a.metadata.clone(),
    }
}

fn parse_bead_status(s: &str) -> Result<BeadStatus, TauriError> {
    let quoted = format!("\"{}\"", s);
    serde_json::from_str(&quoted)
        .map_err(|_| TauriError::InvalidState(format!("unknown bead status: {s}")))
}

fn parse_lane(s: &str) -> Result<Lane, TauriError> {
    let quoted = format!("\"{}\"", s);
    serde_json::from_str(&quoted)
        .map_err(|_| TauriError::InvalidState(format!("unknown lane: {s}")))
}

/// All `BeadStatus` variants, used when listing beads without a status filter.
const ALL_STATUSES: &[BeadStatus] = &[
    BeadStatus::Backlog,
    BeadStatus::Hooked,
    BeadStatus::Slung,
    BeadStatus::Review,
    BeadStatus::Done,
    BeadStatus::Failed,
    BeadStatus::Escalated,
];

// ---------------------------------------------------------------------------
// Status & KPI commands
// ---------------------------------------------------------------------------

/// Return a high-level status snapshot.
pub async fn get_status(state: &AppState) -> Result<StatusResponse, TauriError> {
    let kpi = state.cache.compute_kpi_snapshot().await?;
    let beads_active = kpi.hooked + kpi.slung + kpi.review;
    Ok(StatusResponse {
        agents_active: kpi.active_agents,
        beads_total: kpi.total_beads,
        beads_active,
        cost_today_usd: 0.0, // TODO: wire up cost tracking
        timestamp: Utc::now(),
    })
}

/// Return detailed KPI metrics.
pub async fn get_kpi(state: &AppState) -> Result<KpiResponse, TauriError> {
    let kpi = state.cache.compute_kpi_snapshot().await?;
    Ok(KpiResponse {
        total_beads: kpi.total_beads,
        backlog: kpi.backlog,
        hooked: kpi.hooked,
        slung: kpi.slung,
        review: kpi.review,
        done: kpi.done,
        failed: kpi.failed,
        escalated: kpi.escalated,
        active_agents: kpi.active_agents,
        timestamp: kpi.timestamp,
    })
}

// ---------------------------------------------------------------------------
// Bead commands
// ---------------------------------------------------------------------------

/// List beads, optionally filtered by status string.
pub async fn list_beads(
    state: &AppState,
    status: Option<String>,
) -> Result<Vec<BeadResponse>, TauriError> {
    match status {
        Some(s) => {
            let parsed = parse_bead_status(&s)?;
            let beads = state.cache.list_beads_by_status(parsed).await?;
            Ok(beads.iter().map(bead_to_response).collect())
        }
        None => {
            let mut all = Vec::new();
            for s in ALL_STATUSES {
                let beads = state.cache.list_beads_by_status(s.clone()).await?;
                all.extend(beads.iter().map(bead_to_response));
            }
            Ok(all)
        }
    }
}

/// Get a single bead by ID.
pub async fn get_bead(
    state: &AppState,
    id: String,
) -> Result<BeadResponse, TauriError> {
    let uuid = Uuid::parse_str(&id)
        .map_err(|_| TauriError::InvalidState(format!("invalid uuid: {id}")))?;
    let bead = state
        .cache
        .get_bead(uuid)
        .await?
        .ok_or_else(|| TauriError::NotFound(format!("bead {id}")))?;
    Ok(bead_to_response(&bead))
}

/// Assign a bead to an agent (transition Hooked -> Slung).
pub async fn sling_bead(
    state: &AppState,
    bead_id: String,
    agent_name: String,
) -> Result<BeadResponse, TauriError> {
    let uuid = Uuid::parse_str(&bead_id)
        .map_err(|_| TauriError::InvalidState(format!("invalid uuid: {bead_id}")))?;
    let mut bead = state
        .cache
        .get_bead(uuid)
        .await?
        .ok_or_else(|| TauriError::NotFound(format!("bead {bead_id}")))?;

    if !bead.status.can_transition_to(&BeadStatus::Slung) {
        return Err(TauriError::InvalidState(format!(
            "cannot transition bead from {:?} to slung",
            bead.status
        )));
    }

    let agent = state
        .cache
        .get_agent_by_name(&agent_name)
        .await?
        .ok_or_else(|| TauriError::NotFound(format!("agent {agent_name}")))?;

    bead.status = BeadStatus::Slung;
    bead.agent_id = Some(agent.id);
    bead.slung_at = Some(Utc::now());
    bead.updated_at = Utc::now();

    state.cache.upsert_bead(&bead).await?;
    Ok(bead_to_response(&bead))
}

/// Create a new bead in Hooked status (an agent picks it up immediately).
pub async fn hook_bead(
    state: &AppState,
    title: String,
    agent_name: String,
    lane: Option<String>,
) -> Result<BeadResponse, TauriError> {
    let parsed_lane = match lane {
        Some(l) => parse_lane(&l)?,
        None => Lane::Standard,
    };

    let agent = state
        .cache
        .get_agent_by_name(&agent_name)
        .await?
        .ok_or_else(|| TauriError::NotFound(format!("agent {agent_name}")))?;

    let mut bead = Bead::new(title, parsed_lane);
    bead.status = BeadStatus::Hooked;
    bead.agent_id = Some(agent.id);
    bead.hooked_at = Some(Utc::now());
    bead.updated_at = Utc::now();

    state.cache.upsert_bead(&bead).await?;
    Ok(bead_to_response(&bead))
}

/// Transition a bead to Review status (Slung -> Review).
pub async fn review_bead(
    state: &AppState,
    bead_id: String,
) -> Result<BeadResponse, TauriError> {
    let uuid = Uuid::parse_str(&bead_id)
        .map_err(|_| TauriError::InvalidState(format!("invalid uuid: {bead_id}")))?;
    let mut bead = state
        .cache
        .get_bead(uuid)
        .await?
        .ok_or_else(|| TauriError::NotFound(format!("bead {bead_id}")))?;

    if !bead.status.can_transition_to(&BeadStatus::Review) {
        return Err(TauriError::InvalidState(format!(
            "cannot transition bead from {:?} to review",
            bead.status
        )));
    }

    bead.status = BeadStatus::Review;
    bead.updated_at = Utc::now();

    state.cache.upsert_bead(&bead).await?;
    Ok(bead_to_response(&bead))
}

/// Mark a bead as done or failed.
pub async fn done_bead(
    state: &AppState,
    bead_id: String,
    failed: bool,
) -> Result<BeadResponse, TauriError> {
    let uuid = Uuid::parse_str(&bead_id)
        .map_err(|_| TauriError::InvalidState(format!("invalid uuid: {bead_id}")))?;
    let mut bead = state
        .cache
        .get_bead(uuid)
        .await?
        .ok_or_else(|| TauriError::NotFound(format!("bead {bead_id}")))?;

    let target = if failed {
        BeadStatus::Failed
    } else {
        BeadStatus::Done
    };

    if !bead.status.can_transition_to(&target) {
        return Err(TauriError::InvalidState(format!(
            "cannot transition bead from {:?} to {:?}",
            bead.status, target
        )));
    }

    bead.status = target;
    bead.done_at = Some(Utc::now());
    bead.updated_at = Utc::now();

    state.cache.upsert_bead(&bead).await?;
    Ok(bead_to_response(&bead))
}

// ---------------------------------------------------------------------------
// Agent commands
// ---------------------------------------------------------------------------

/// List all agents by querying each known status.
pub async fn list_agents(state: &AppState) -> Result<Vec<AgentResponse>, TauriError> {
    // CacheDb does not expose a "list all agents" method, so we use the KPI
    // snapshot to check if there are any agents, and gather them by querying
    // individual known agents. For a real implementation this would need a
    // list_agents method on CacheDb. For now, return an empty vec since we
    // cannot enumerate agents without such a method.
    //
    // NOTE: This is a known limitation -- the full implementation will add
    // a `list_agents` query to CacheDb.
    let _ = state;
    Ok(Vec::new())
}

/// Get a single agent by name.
pub async fn get_agent(
    state: &AppState,
    name: String,
) -> Result<AgentResponse, TauriError> {
    let agent = state
        .cache
        .get_agent_by_name(&name)
        .await?
        .ok_or_else(|| TauriError::NotFound(format!("agent {name}")))?;
    Ok(agent_to_response(&agent))
}

/// Send a nudge message to an agent via the event bus.
pub async fn nudge_agent(
    state: &AppState,
    name: String,
    message: String,
) -> Result<(), TauriError> {
    // Verify the agent exists.
    let _agent = state
        .cache
        .get_agent_by_name(&name)
        .await?
        .ok_or_else(|| TauriError::NotFound(format!("agent {name}")))?;

    use at_bridge::protocol::BridgeMessage;
    state.event_bus.publish(BridgeMessage::NudgeAgent {
        agent_name: name,
        message,
    });
    Ok(())
}
