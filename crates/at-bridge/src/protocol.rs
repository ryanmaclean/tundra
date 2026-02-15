use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
#[serde(rename_all = "snake_case")]
pub enum BridgeMessage {
    // Frontend -> Backend
    GetStatus,
    ListBeads { status: Option<String> },
    ListAgents,
    SlingBead { bead_id: Uuid, agent_id: Uuid },
    HookBead { title: String, agent_name: String },
    DoneBead { bead_id: Uuid, failed: bool },
    NudgeAgent { agent_name: String, message: String },
    GetKpi,

    // Backend -> Frontend
    StatusUpdate(StatusPayload),
    BeadList(Vec<at_core::types::Bead>),
    AgentList(Vec<at_core::types::Agent>),
    KpiUpdate(KpiPayload),
    AgentOutput { agent_id: Uuid, output: String },
    Error { code: String, message: String },
    Event(EventPayload),
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
