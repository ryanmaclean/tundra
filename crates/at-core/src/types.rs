use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// BeadStatus
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeadStatus {
    Backlog,
    Hooked,
    Slung,
    Review,
    Done,
    Failed,
    Escalated,
}

impl BeadStatus {
    /// Returns `true` when a transition from `self` to `target` is valid.
    pub fn can_transition_to(&self, target: &BeadStatus) -> bool {
        matches!(
            (self, target),
            (BeadStatus::Backlog, BeadStatus::Hooked)
                | (BeadStatus::Hooked, BeadStatus::Slung)
                | (BeadStatus::Hooked, BeadStatus::Backlog)
                | (BeadStatus::Slung, BeadStatus::Review)
                | (BeadStatus::Slung, BeadStatus::Failed)
                | (BeadStatus::Slung, BeadStatus::Escalated)
                | (BeadStatus::Review, BeadStatus::Done)
                | (BeadStatus::Review, BeadStatus::Slung)
                | (BeadStatus::Review, BeadStatus::Failed)
                | (BeadStatus::Failed, BeadStatus::Backlog)
                | (BeadStatus::Escalated, BeadStatus::Backlog)
        )
    }
}

// ---------------------------------------------------------------------------
// Lane
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    Experimental = 0,
    Standard = 1,
    Critical = 2,
}

// ---------------------------------------------------------------------------
// Bead
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bead {
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

impl Bead {
    pub fn new(title: impl Into<String>, lane: Lane) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            description: None,
            status: BeadStatus::Backlog,
            lane,
            priority: 0,
            agent_id: None,
            convoy_id: None,
            created_at: now,
            updated_at: now,
            hooked_at: None,
            slung_at: None,
            done_at: None,
            git_branch: None,
            metadata: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Agent-related enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Mayor,
    Deacon,
    Witness,
    Refinery,
    Polecat,
    Crew,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CliType {
    Claude,
    Codex,
    Gemini,
    OpenCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Active,
    Idle,
    Pending,
    Unknown,
    Stopped,
}

impl AgentStatus {
    pub fn glyph(&self) -> &'static str {
        match self {
            AgentStatus::Active => "@",
            AgentStatus::Idle => "*",
            AgentStatus::Pending => "!",
            AgentStatus::Unknown => "?",
            AgentStatus::Stopped => "x",
        }
    }
}

// ---------------------------------------------------------------------------
// Agent
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
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

impl Agent {
    pub fn new(name: impl Into<String>, role: AgentRole, cli_type: CliType) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            role,
            cli_type,
            model: None,
            status: AgentStatus::Pending,
            rig: None,
            pid: None,
            session_id: None,
            created_at: now,
            last_seen: now,
            metadata: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Convoy
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConvoyStatus {
    Forming,
    Active,
    Completed,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Convoy {
    pub id: Uuid,
    pub name: String,
    pub status: ConvoyStatus,
    pub bead_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Mail
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mail {
    pub id: Uuid,
    pub from_agent: Uuid,
    pub to_agent: Uuid,
    pub subject: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub read: bool,
}

// ---------------------------------------------------------------------------
// Event
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: Uuid,
    pub kind: String,
    pub source: String,
    pub payload: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// TokenMetric
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetric {
    pub agent_id: Uuid,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// KpiSnapshot
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiSnapshot {
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
