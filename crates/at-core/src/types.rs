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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    // --- Core orchestration ---
    Mayor,
    Deacon,
    Witness,
    Refinery,
    Polecat,
    Crew,

    // --- Spec pipeline ---
    SpecGatherer,
    SpecWriter,
    SpecResearcher,
    SpecCritic,
    SpecValidator,

    // --- Planning ---
    Planner,
    FollowupPlanner,

    // --- Coding ---
    Coder,
    CoderRecovery,

    // --- QA ---
    QaReviewer,
    QaFixer,
    ValidationFixer,

    // --- Analysis ---
    InsightExtractor,
    ComplexityAssessor,
    CompetitorAnalysis,
    AiAnalyzer,

    // --- Ideation ---
    IdeationCodeQuality,
    IdeationPerformance,
    IdeationSecurity,
    IdeationDocumentation,
    IdeationUiUx,
    IdeationCodeImprovements,

    // --- Roadmap ---
    RoadmapDiscovery,
    RoadmapFeatures,

    // --- Utilities ---
    CommitMessage,
    PrTemplateFiller,
    MergeResolver,

    // --- Dynamic plugin agent (from .claude/agents/) ---
    Plugin,
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
// TaskImpact
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskImpact {
    Low,
    Medium,
    High,
    Critical,
}

// ---------------------------------------------------------------------------
// PhaseConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhaseConfig {
    pub phase_name: String,
    pub model: String,
    pub thinking_level: String,
}

impl Default for PhaseConfig {
    fn default() -> Self {
        Self {
            phase_name: "spec_creation".to_string(),
            model: "sonnet".to_string(),
            thinking_level: "medium".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// AgentProfile
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentProfile {
    Auto,
    Complex,
    Balanced,
    Quick,
    Custom(String),
}

impl AgentProfile {
    /// Return a human-readable display name for the profile.
    pub fn display_name(&self) -> &str {
        match self {
            AgentProfile::Auto => "Auto Optimized",
            AgentProfile::Complex => "Complex",
            AgentProfile::Balanced => "Balanced",
            AgentProfile::Quick => "Quick",
            AgentProfile::Custom(_) => "Custom",
        }
    }

    /// Return default phase configurations for this profile.
    pub fn default_phase_configs(&self) -> Vec<PhaseConfig> {
        match self {
            AgentProfile::Auto => vec![
                PhaseConfig { phase_name: "spec_creation".into(), model: "sonnet".into(), thinking_level: "medium".into() },
                PhaseConfig { phase_name: "planning".into(), model: "sonnet".into(), thinking_level: "medium".into() },
                PhaseConfig { phase_name: "code_review".into(), model: "sonnet".into(), thinking_level: "medium".into() },
            ],
            AgentProfile::Complex => vec![
                PhaseConfig { phase_name: "spec_creation".into(), model: "opus".into(), thinking_level: "high".into() },
                PhaseConfig { phase_name: "planning".into(), model: "opus".into(), thinking_level: "high".into() },
                PhaseConfig { phase_name: "code_review".into(), model: "opus".into(), thinking_level: "high".into() },
            ],
            AgentProfile::Balanced => vec![
                PhaseConfig { phase_name: "spec_creation".into(), model: "sonnet".into(), thinking_level: "medium".into() },
                PhaseConfig { phase_name: "planning".into(), model: "opus".into(), thinking_level: "medium".into() },
                PhaseConfig { phase_name: "code_review".into(), model: "haiku".into(), thinking_level: "low".into() },
            ],
            AgentProfile::Quick => vec![
                PhaseConfig { phase_name: "spec_creation".into(), model: "haiku".into(), thinking_level: "low".into() },
                PhaseConfig { phase_name: "planning".into(), model: "haiku".into(), thinking_level: "low".into() },
                PhaseConfig { phase_name: "code_review".into(), model: "haiku".into(), thinking_level: "low".into() },
            ],
            AgentProfile::Custom(_) => vec![
                PhaseConfig { phase_name: "spec_creation".into(), model: "sonnet".into(), thinking_level: "medium".into() },
                PhaseConfig { phase_name: "planning".into(), model: "sonnet".into(), thinking_level: "medium".into() },
                PhaseConfig { phase_name: "code_review".into(), model: "sonnet".into(), thinking_level: "medium".into() },
            ],
        }
    }
}

impl std::fmt::Display for AgentProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
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
// TaskSource
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskSource {
    Manual,
    GithubIssue { issue_number: u32 },
    GithubPr { pr_number: u32 },
    GitlabIssue { iid: u32 },
    LinearIssue { identifier: String },
    Import,
    Ideation { idea_id: String },
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
    pub impact: Option<TaskImpact>,
    pub agent_profile: Option<AgentProfile>,
    pub phase_configs: Vec<PhaseConfig>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub logs: Vec<TaskLogEntry>,
    pub qa_report: Option<QaReport>,
    #[serde(default)]
    pub source: Option<TaskSource>,
    /// Parent task ID for stacked diffs (None = root or standalone).
    #[serde(default)]
    pub parent_task_id: Option<Uuid>,
    /// Position within the stack (0 = first child).
    #[serde(default)]
    pub stack_position: Option<u32>,
    /// Associated pull-request number.
    #[serde(default)]
    pub pr_number: Option<u32>,
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
            impact: None,
            agent_profile: None,
            phase_configs: Vec::new(),
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            error: None,
            logs: Vec::new(),
            qa_report: None,
            source: None,
            parent_task_id: None,
            stack_position: None,
            pr_number: None,
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
// QA Report types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QaSeverity {
    Critical,
    Major,
    Minor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QaStatus {
    Passed,
    Failed,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaIssue {
    pub id: Uuid,
    pub severity: QaSeverity,
    pub description: String,
    pub file: Option<String>,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaReport {
    pub id: Uuid,
    pub task_id: Uuid,
    pub status: QaStatus,
    pub issues: Vec<QaIssue>,
    pub timestamp: DateTime<Utc>,
}

impl QaReport {
    pub fn new(task_id: Uuid, status: QaStatus) -> Self {
        Self {
            id: Uuid::new_v4(),
            task_id,
            status,
            issues: Vec::new(),
            timestamp: Utc::now(),
        }
    }

    /// Returns true if any issue has Critical severity.
    pub fn has_critical_issues(&self) -> bool {
        self.issues.iter().any(|i| i.severity == QaSeverity::Critical)
    }

    /// Determine the next phase based on QA status.
    pub fn next_phase(&self) -> TaskPhase {
        match self.status {
            QaStatus::Passed => TaskPhase::Merging,
            QaStatus::Failed => TaskPhase::Fixing,
            QaStatus::Pending => TaskPhase::Qa,
        }
    }
}

// ---------------------------------------------------------------------------
// TaskFile types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskFileType {
    Spec,
    Implementation,
    Test,
    Config,
    Documentation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFile {
    pub id: Uuid,
    pub task_id: Uuid,
    pub path: String,
    pub file_type: TaskFileType,
    pub content: Option<String>,
    pub size_bytes: Option<u64>,
    pub phase_added: TaskPhase,
    pub subtask_id: Option<Uuid>,
}

impl TaskFile {
    pub fn new(
        task_id: Uuid,
        path: impl Into<String>,
        file_type: TaskFileType,
        phase: TaskPhase,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            task_id,
            path: path.into(),
            file_type,
            content: None,
            size_bytes: None,
            phase_added: phase,
            subtask_id: None,
        }
    }

    /// Normalize the file path (remove leading ./, collapse //)
    pub fn normalized_path(&self) -> String {
        let p = self.path.replace("//", "/");
        p.strip_prefix("./").unwrap_or(&p).to_string()
    }
}

// ---------------------------------------------------------------------------
// TaskFiles collection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskFiles {
    pub files: Vec<TaskFile>,
}

impl TaskFiles {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    pub fn add(&mut self, file: TaskFile) {
        self.files.push(file);
    }

    /// Check if a file path already exists in the collection.
    pub fn has_path(&self, path: &str) -> bool {
        self.files.iter().any(|f| f.path == path)
    }

    /// Filter files by type.
    pub fn by_type(&self, file_type: &TaskFileType) -> Vec<&TaskFile> {
        self.files.iter().filter(|f| &f.file_type == file_type).collect()
    }

    /// Filter files by phase.
    pub fn by_phase(&self, phase: &TaskPhase) -> Vec<&TaskFile> {
        self.files.iter().filter(|f| &f.phase_added == phase).collect()
    }

    /// Filter files by subtask.
    pub fn by_subtask(&self, subtask_id: Uuid) -> Vec<&TaskFile> {
        self.files.iter().filter(|f| f.subtask_id == Some(subtask_id)).collect()
    }

    /// Count of files.
    pub fn count(&self) -> usize {
        self.files.len()
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
