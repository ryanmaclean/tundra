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
// TaskPhase
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPhase {
    Discovery,
    ContextGathering,
    SpecCreation,
    Planning,
    Coding,
    Qa,
    Fixing,
    Merging,
    Complete,
    Error,
    Stopped,
}

impl TaskPhase {
    /// Returns `true` when a transition from `self` to `target` is valid.
    pub fn can_transition_to(&self, target: &TaskPhase) -> bool {
        matches!(
            (self, target),
            (TaskPhase::Discovery, TaskPhase::ContextGathering)
                | (TaskPhase::ContextGathering, TaskPhase::SpecCreation)
                | (TaskPhase::SpecCreation, TaskPhase::Planning)
                | (TaskPhase::Planning, TaskPhase::Coding)
                | (TaskPhase::Coding, TaskPhase::Qa)
                | (TaskPhase::Qa, TaskPhase::Fixing)
                | (TaskPhase::Qa, TaskPhase::Merging)
                | (TaskPhase::Fixing, TaskPhase::Qa)
                | (TaskPhase::Fixing, TaskPhase::Coding)
                | (TaskPhase::Merging, TaskPhase::Complete)
                // Any phase can transition to Error or Stopped
                | (_, TaskPhase::Error)
                | (_, TaskPhase::Stopped)
        )
    }

    /// The ordered pipeline phases (excluding Error/Stopped terminal states).
    pub fn pipeline_order() -> &'static [TaskPhase] {
        &[
            TaskPhase::Discovery,
            TaskPhase::ContextGathering,
            TaskPhase::SpecCreation,
            TaskPhase::Planning,
            TaskPhase::Coding,
            TaskPhase::Qa,
            TaskPhase::Fixing,
            TaskPhase::Merging,
            TaskPhase::Complete,
        ]
    }

    /// Approximate progress percentage for this phase.
    pub fn progress_percent(&self) -> u8 {
        match self {
            TaskPhase::Discovery => 5,
            TaskPhase::ContextGathering => 15,
            TaskPhase::SpecCreation => 25,
            TaskPhase::Planning => 35,
            TaskPhase::Coding => 55,
            TaskPhase::Qa => 70,
            TaskPhase::Fixing => 80,
            TaskPhase::Merging => 90,
            TaskPhase::Complete => 100,
            TaskPhase::Error => 0,
            TaskPhase::Stopped => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// TaskCategory / TaskPriority / TaskComplexity
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCategory {
    Feature,
    BugFix,
    Refactoring,
    Documentation,
    Security,
    Performance,
    UiUx,
    Infrastructure,
    Testing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Low,
    Medium,
    High,
    Urgent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskComplexity {
    Trivial,
    Small,
    Medium,
    Large,
    Complex,
}

// ---------------------------------------------------------------------------
// SubtaskStatus / Subtask
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtaskStatus {
    Pending,
    InProgress,
    Complete,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    pub id: Uuid,
    pub title: String,
    pub status: SubtaskStatus,
    pub agent_id: Option<Uuid>,
    pub depends_on: Vec<Uuid>,
}

// ---------------------------------------------------------------------------
// TaskLogType / TaskLogEntry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskLogType {
    Text,
    PhaseStart,
    PhaseEnd,
    ToolStart,
    ToolEnd,
    Error,
    Success,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLogEntry {
    pub timestamp: DateTime<Utc>,
    pub phase: TaskPhase,
    pub log_type: TaskLogType,
    pub message: String,
    pub detail: Option<String>,
}

// ---------------------------------------------------------------------------
// Task
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub bead_id: Uuid,
    pub phase: TaskPhase,
    pub progress_percent: u8,
    pub subtasks: Vec<Subtask>,
    pub worktree_path: Option<String>,
    pub git_branch: Option<String>,
    pub category: TaskCategory,
    pub priority: TaskPriority,
    pub complexity: TaskComplexity,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub logs: Vec<TaskLogEntry>,
}

impl Task {
    pub fn new(
        title: impl Into<String>,
        bead_id: Uuid,
        category: TaskCategory,
        priority: TaskPriority,
        complexity: TaskComplexity,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            description: None,
            bead_id,
            phase: TaskPhase::Discovery,
            progress_percent: 0,
            subtasks: Vec::new(),
            worktree_path: None,
            git_branch: None,
            category,
            priority,
            complexity,
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            error: None,
            logs: Vec::new(),
        }
    }

    /// Append a log entry for the current phase.
    pub fn log(&mut self, log_type: TaskLogType, message: impl Into<String>) {
        self.logs.push(TaskLogEntry {
            timestamp: Utc::now(),
            phase: self.phase.clone(),
            log_type,
            message: message.into(),
            detail: None,
        });
        self.updated_at = Utc::now();
    }

    /// Transition the task to a new phase, updating progress.
    pub fn set_phase(&mut self, phase: TaskPhase) {
        self.progress_percent = phase.progress_percent();
        self.phase = phase;
        self.updated_at = Utc::now();
    }
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
