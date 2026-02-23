use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
#[serde(rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum BridgeMessage {
    // Frontend -> Backend
    GetStatus,
    ListBeads {
        status: Option<String>,
    },
    ListAgents,
    SlingBead {
        bead_id: Uuid,
        agent_id: Uuid,
    },
    HookBead {
        title: String,
        agent_name: String,
    },
    DoneBead {
        bead_id: Uuid,
        failed: bool,
    },
    NudgeAgent {
        agent_name: String,
        message: String,
    },
    GetKpi,

    // Backend -> Frontend
    StatusUpdate(StatusPayload),
    BeadList(Vec<at_core::types::Bead>),
    AgentList(Vec<at_core::types::Agent>),
    KpiUpdate(KpiPayload),
    AgentOutput {
        agent_id: Uuid,
        output: String,
    },
    Error {
        code: String,
        message: String,
    },
    Event(EventPayload),
    /// Real-time task update (phase change, progress, subtasks). Subscribe on /api/events/ws.
    TaskUpdate(Box<at_core::types::Task>),
    /// Merge completed or conflict detected on a worktree branch.
    MergeResult {
        worktree_id: String,
        branch: String,
        status: String,
        conflict_files: Vec<String>,
    },
    /// Queue reordering or priority change.
    QueueUpdate {
        task_ids: Vec<Uuid>,
    },
    /// Bead created event.
    BeadCreated(at_core::types::Bead),
    /// Bead updated event.
    BeadUpdated(at_core::types::Bead),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusPayload {
    pub version: String,
    pub uptime_seconds: u64,
    pub agents_active: u32,
    pub beads_active: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiPayload {
    pub total_beads: u64,
    pub backlog: u64,
    pub hooked: u64,
    pub slung: u64,
    pub review: u64,
    pub done: u64,
    pub failed: u64,
    pub active_agents: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    pub event_type: String,
    pub agent_id: Option<Uuid>,
    pub bead_id: Option<Uuid>,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
