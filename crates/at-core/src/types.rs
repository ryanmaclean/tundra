use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// BeadStatus
// ---------------------------------------------------------------------------

/// Lifecycle status of a Bead as it moves through the workflow.
///
/// Valid transitions are enforced by the `can_transition_to` method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeadStatus {
    /// Queued and waiting to be picked up.
    Backlog,
    /// Assigned to an agent, not yet started.
    Hooked,
    /// Actively being worked on.
    Slung,
    /// Awaiting code review or QA.
    Review,
    /// Successfully completed.
    Done,
    /// Encountered an error or failure.
    Failed,
    /// Requires human intervention.
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

/// Priority lane for routing and scheduling Beads.
///
/// Higher lanes (Critical > Standard > Experimental) are processed first.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    /// Low priority, experimental features.
    Experimental = 0,
    /// Normal priority, standard workflow.
    Standard = 1,
    /// High priority, urgent items.
    Critical = 2,
}

// ---------------------------------------------------------------------------
// Bead
// ---------------------------------------------------------------------------

/// A unit of work in the Tundra system, representing a task or feature.
///
/// Beads move through a defined lifecycle (`BeadStatus`) and can be routed
/// via different priority lanes (`Lane`). They can be assigned to agents
/// and grouped into convoys for coordinated multi-task execution.
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
    /// Create a new Bead in Backlog status with the given title and lane.
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

/// Specialized role defining an agent's responsibilities in the Tundra system.
///
/// Agents are assigned specific roles that determine their capabilities and
/// which tasks they can handle. Roles are organized by functional area:
/// orchestration, spec pipeline, planning, coding, QA, analysis, ideation,
/// roadmap, and utilities.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    // --- Core orchestration ---
    /// Top-level orchestrator managing task distribution and workflow.
    Mayor,
    /// Coordinates agent communication and event propagation.
    Deacon,
    /// Monitors system health and agent activity.
    Witness,
    /// Processes and refines task metadata and context.
    Refinery,
    /// Handles error recovery and task escalation.
    Polecat,
    /// General-purpose worker agent for flexible task execution.
    Crew,

    // --- Spec pipeline ---
    /// Collects requirements and context for spec creation.
    SpecGatherer,
    /// Writes technical specifications from gathered requirements.
    SpecWriter,
    /// Researches external context and dependencies for specs.
    SpecResearcher,
    /// Reviews and critiques spec quality and completeness.
    SpecCritic,
    /// Validates specs against acceptance criteria.
    SpecValidator,

    // --- Planning ---
    /// Creates implementation plans from specifications.
    Planner,
    /// Generates follow-up plans for additional work or refinements.
    FollowupPlanner,

    // --- Coding ---
    /// Executes code implementation from plans.
    Coder,
    /// Handles error recovery during coding phase.
    CoderRecovery,

    // --- QA ---
    /// Reviews code quality and runs QA checks.
    QaReviewer,
    /// Fixes issues identified during QA review.
    QaFixer,
    /// Addresses validation failures in the pipeline.
    ValidationFixer,

    // --- Analysis ---
    /// Extracts insights and metrics from codebase or tasks.
    InsightExtractor,
    /// Assesses task complexity and effort estimates.
    ComplexityAssessor,
    /// Analyzes competitor features and implementations.
    CompetitorAnalysis,
    /// Performs AI-driven code and pattern analysis.
    AiAnalyzer,

    // --- Ideation ---
    /// Generates ideas for code quality improvements.
    IdeationCodeQuality,
    /// Generates ideas for performance optimizations.
    IdeationPerformance,
    /// Generates ideas for security enhancements.
    IdeationSecurity,
    /// Generates ideas for documentation improvements.
    IdeationDocumentation,
    /// Generates ideas for UI/UX enhancements.
    IdeationUiUx,
    /// Generates general code improvement suggestions.
    IdeationCodeImprovements,

    // --- Roadmap ---
    /// Discovers and analyzes roadmap opportunities.
    RoadmapDiscovery,
    /// Defines and prioritizes roadmap features.
    RoadmapFeatures,

    // --- Utilities ---
    /// Generates commit messages from code changes.
    CommitMessage,
    /// Fills pull request templates with task context.
    PrTemplateFiller,
    /// Resolves merge conflicts automatically.
    MergeResolver,

    // --- Dynamic plugin agent (from .claude/agents/) ---
    /// Custom plugin agent loaded from `.claude/agents/` directory.
    Plugin,
}

/// AI CLI provider type used by an agent for execution.
///
/// Determines which AI assistant CLI the agent uses to interact with
/// language models and execute tasks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CliType {
    /// Claude AI assistant (Anthropic).
    Claude,
    /// Codex AI assistant (OpenAI).
    Codex,
    /// Gemini AI assistant (Google).
    Gemini,
    /// OpenCode AI assistant.
    OpenCode,
}

/// Current operational status of an agent.
///
/// Tracks whether an agent is actively working, idle, or unavailable.
/// Used for agent health monitoring and task routing decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// Agent is currently executing a task.
    Active,
    /// Agent is online but not currently assigned work.
    Idle,
    /// Agent is starting up or initializing.
    Pending,
    /// Agent status cannot be determined.
    Unknown,
    /// Agent has been stopped or shut down.
    Stopped,
}

impl AgentStatus {
    /// Returns a single-character glyph representing the status.
    ///
    /// Used for compact status display in logs and UI.
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

/// An autonomous AI worker in the Tundra system.
///
/// Agents execute specialized tasks based on their assigned `role` and can be
/// tracked across their lifecycle. Each agent has a unique name and uses a
/// specific AI CLI provider (`cli_type`) to interact with language models.
///
/// Agents can be assigned to beads, monitored for health, and coordinated
/// through the orchestration layer.
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
    /// Create a new Agent in Pending status with the given name, role, and CLI type.
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

/// Lifecycle status of a Convoy as it coordinates multiple Beads.
///
/// Convoys group related beads together for coordinated execution,
/// progressing through formation, execution, and completion phases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConvoyStatus {
    /// Convoy is being assembled, beads are being added.
    Forming,
    /// Convoy is actively executing its grouped beads.
    Active,
    /// All beads in the convoy have finished successfully.
    Completed,
    /// Convoy execution was cancelled or failed.
    Aborted,
}

/// A coordinated group of Beads executed together as a unit.
///
/// Convoys enable multi-task coordination where related beads need to be
/// processed as a cohesive set. This is useful for features that span
/// multiple sub-tasks or require synchronized execution across agents.
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

/// A message sent between agents for coordination and communication.
///
/// Mail enables asynchronous inter-agent messaging, allowing agents to
/// share context, request assistance, or coordinate on shared tasks.
/// Messages are tracked by read status and support structured content
/// via subject/body fields.
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

/// A system event capturing state changes and significant actions.
///
/// Events provide an audit trail and enable event-driven coordination
/// between system components. Each event has a kind (e.g., "bead.hooked",
/// "agent.started"), a source identifier, and a flexible JSON payload
/// for event-specific data.
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

/// Token usage and cost tracking for LLM API calls by an agent.
///
/// Captures input/output token counts and estimated costs for billing
/// and usage analytics. Metrics are associated with specific agents
/// to enable per-agent cost tracking and optimization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetric {
    pub agent_id: Uuid,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// BuildStream / BuildLogEntry
// ---------------------------------------------------------------------------

/// Identifies whether a build log line came from stdout or stderr.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildStream {
    Stdout,
    Stderr,
}

/// A single captured line of build output (stdout or stderr) with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildLogEntry {
    pub timestamp: DateTime<Utc>,
    pub stream: BuildStream,
    pub line: String,
    pub phase: TaskPhase,
}

// ---------------------------------------------------------------------------
// TaskPhase
// ---------------------------------------------------------------------------

/// Execution phase of a Task as it moves through the pipeline.
///
/// Tasks progress through a defined pipeline from discovery to completion.
/// Valid transitions are enforced by the `can_transition_to` method.
/// The `pipeline_order` method provides the canonical phase sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPhase {
    /// Initial phase: discovering requirements and context.
    Discovery,
    /// Gathering additional context and background information.
    ContextGathering,
    /// Creating a technical specification document.
    SpecCreation,
    /// Generating an implementation plan from the spec.
    Planning,
    /// Actively implementing code changes.
    Coding,
    /// Running quality assurance checks and code review.
    Qa,
    /// Addressing issues found during QA.
    Fixing,
    /// Merging changes into the target branch.
    Merging,
    /// Task successfully completed.
    Complete,
    /// Task encountered an unrecoverable error.
    Error,
    /// Task was manually stopped or cancelled.
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

/// Classification of a Task by its functional purpose.
///
/// Categories help with routing tasks to appropriate agents, reporting,
/// and prioritization. They describe what kind of work the task represents
/// rather than its urgency or size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCategory {
    /// New functionality or capability.
    Feature,
    /// Fixing a defect or incorrect behavior.
    BugFix,
    /// Code restructuring without behavior changes.
    Refactoring,
    /// Adding or improving documentation.
    Documentation,
    /// Security-related improvements or fixes.
    Security,
    /// Performance optimization work.
    Performance,
    /// User interface or user experience improvements.
    UiUx,
    /// Infrastructure, tooling, or build system changes.
    Infrastructure,
    /// Test creation or improvement.
    Testing,
}

/// Priority level of a Task for scheduling and execution order.
///
/// Higher priority tasks are processed before lower priority ones.
/// Priority is independent of complexity or impact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    /// Low urgency, can be deferred.
    Low,
    /// Normal priority, standard workflow.
    Medium,
    /// Important, should be addressed soon.
    High,
    /// Critical urgency, needs immediate attention.
    Urgent,
}

/// Estimated complexity and effort required for a Task.
///
/// Complexity assessment helps with time estimation, agent assignment,
/// and resource planning. Larger complexity may trigger more thorough
/// planning or assignment to more capable agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskComplexity {
    /// Very simple, quick change (minutes).
    Trivial,
    /// Small task, straightforward implementation (< 1 hour).
    Small,
    /// Moderate effort, some complexity (1-4 hours).
    Medium,
    /// Significant work, multiple components (4-8 hours).
    Large,
    /// Complex task requiring careful planning (> 8 hours).
    Complex,
}

// ---------------------------------------------------------------------------
// TaskImpact
// ---------------------------------------------------------------------------

/// Expected impact of a Task on the system or users.
///
/// Impact assessment helps prioritize work and determine appropriate
/// QA rigor. Higher impact tasks may require more thorough testing,
/// additional review, or special deployment considerations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskImpact {
    /// Minimal impact, isolated change.
    Low,
    /// Moderate impact, affects specific features or areas.
    Medium,
    /// Significant impact, affects major functionality or many users.
    High,
    /// Critical impact, affects core systems or all users.
    Critical,
}

// ---------------------------------------------------------------------------
// PhaseConfig
// ---------------------------------------------------------------------------

/// Configuration for a specific execution phase in an agent workflow.
///
/// Each phase (e.g., spec_creation, planning, code_review) can be configured
/// with a specific model and thinking level to optimize cost and performance.
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
// RetentionConfig
// ---------------------------------------------------------------------------

/// Configuration for data retention and cleanup policies.
///
/// Controls how long various in-memory data structures are retained before
/// being cleaned up to prevent unbounded memory growth. This includes task
/// archival, build log truncation, orchestrator execution history, and
/// disconnect buffer cleanup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionConfig {
    /// Time-to-live for archived tasks in seconds (default: 7 days).
    ///
    /// Tasks that have been completed and archived will be removed from
    /// memory after this duration expires.
    pub task_ttl_secs: u64,

    /// Maximum number of build log entries to retain per task (default: 10,000).
    ///
    /// When a task's build logs exceed this limit, older entries are truncated
    /// to prevent unbounded growth during long-running builds.
    pub max_task_logs: usize,

    /// Interval between cleanup runs in seconds (default: 1 hour).
    ///
    /// Background cleanup tasks will run at this interval to remove expired
    /// data from memory.
    pub cleanup_interval_secs: u64,

    /// Time-to-live for orchestrator execution history in seconds (default: 24 hours).
    ///
    /// Orchestrator execution records, decompositions, and refinements are
    /// removed after this duration to prevent the history HashMaps from
    /// growing indefinitely.
    pub orchestrator_execution_ttl_secs: u64,

    /// Time-to-live for terminal disconnect buffers in seconds (default: 5 minutes).
    ///
    /// Disconnect buffers that have exceeded this TTL without reconnection
    /// will be cleaned up to free memory.
    pub disconnect_buffer_ttl_secs: u64,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            task_ttl_secs: 7 * 24 * 60 * 60,        // 7 days
            max_task_logs: 10_000,                   // 10k entries
            cleanup_interval_secs: 60 * 60,          // 1 hour
            orchestrator_execution_ttl_secs: 24 * 60 * 60, // 24 hours
            disconnect_buffer_ttl_secs: 5 * 60,      // 5 minutes
        }
    }
}

// ---------------------------------------------------------------------------
// AgentProfile
// ---------------------------------------------------------------------------

/// Execution profile defining model selection and thinking levels for agents.
///
/// Profiles optimize the trade-off between speed, cost, and quality by selecting
/// appropriate models and thinking levels for each phase of execution. Auto mode
/// adapts based on task complexity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentProfile {
    /// Automatically selects optimal configuration based on task complexity.
    Auto,
    /// High-quality execution using Opus model with high thinking levels.
    Complex,
    /// Balanced approach mixing Opus for planning with faster models elsewhere.
    Balanced,
    /// Fast execution using Haiku model with low thinking levels.
    Quick,
    /// User-defined custom profile with specified configuration.
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
                PhaseConfig {
                    phase_name: "spec_creation".into(),
                    model: "sonnet".into(),
                    thinking_level: "medium".into(),
                },
                PhaseConfig {
                    phase_name: "planning".into(),
                    model: "sonnet".into(),
                    thinking_level: "medium".into(),
                },
                PhaseConfig {
                    phase_name: "code_review".into(),
                    model: "sonnet".into(),
                    thinking_level: "medium".into(),
                },
            ],
            AgentProfile::Complex => vec![
                PhaseConfig {
                    phase_name: "spec_creation".into(),
                    model: "opus".into(),
                    thinking_level: "high".into(),
                },
                PhaseConfig {
                    phase_name: "planning".into(),
                    model: "opus".into(),
                    thinking_level: "high".into(),
                },
                PhaseConfig {
                    phase_name: "code_review".into(),
                    model: "opus".into(),
                    thinking_level: "high".into(),
                },
            ],
            AgentProfile::Balanced => vec![
                PhaseConfig {
                    phase_name: "spec_creation".into(),
                    model: "sonnet".into(),
                    thinking_level: "medium".into(),
                },
                PhaseConfig {
                    phase_name: "planning".into(),
                    model: "opus".into(),
                    thinking_level: "medium".into(),
                },
                PhaseConfig {
                    phase_name: "code_review".into(),
                    model: "haiku".into(),
                    thinking_level: "low".into(),
                },
            ],
            AgentProfile::Quick => vec![
                PhaseConfig {
                    phase_name: "spec_creation".into(),
                    model: "haiku".into(),
                    thinking_level: "low".into(),
                },
                PhaseConfig {
                    phase_name: "planning".into(),
                    model: "haiku".into(),
                    thinking_level: "low".into(),
                },
                PhaseConfig {
                    phase_name: "code_review".into(),
                    model: "haiku".into(),
                    thinking_level: "low".into(),
                },
            ],
            AgentProfile::Custom(_) => vec![
                PhaseConfig {
                    phase_name: "spec_creation".into(),
                    model: "sonnet".into(),
                    thinking_level: "medium".into(),
                },
                PhaseConfig {
                    phase_name: "planning".into(),
                    model: "sonnet".into(),
                    thinking_level: "medium".into(),
                },
                PhaseConfig {
                    phase_name: "code_review".into(),
                    model: "sonnet".into(),
                    thinking_level: "medium".into(),
                },
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

/// Execution status of a Subtask within a Task.
///
/// Subtasks progress from Pending through InProgress to a terminal state
/// (Complete, Failed, or Skipped). This status tracking enables granular
/// progress monitoring within larger tasks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtaskStatus {
    /// Queued and waiting to be started.
    Pending,
    /// Currently being executed.
    InProgress,
    /// Successfully completed.
    Complete,
    /// Execution failed or encountered an error.
    Failed,
    /// Intentionally skipped (e.g., due to dependencies or conditions).
    Skipped,
}

/// A granular unit of work within a Task, enabling detailed execution tracking.
///
/// Subtasks break down complex tasks into manageable steps, each with its own
/// status and optional agent assignment. Dependencies between subtasks can be
/// expressed via `depends_on`, enabling ordered execution and parallel work
/// where possible.
///
/// Subtasks are particularly useful during the Coding and QA phases where
/// implementation plans specify multiple discrete steps that need individual
/// tracking and validation.
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

/// Classification of a task log entry by its semantic purpose.
///
/// Log types enable structured filtering and presentation of task execution
/// history. They distinguish between phase transitions, tool invocations,
/// status updates, and diagnostic messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskLogType {
    /// General text message or narrative update.
    Text,
    /// Marks the beginning of a new pipeline phase.
    PhaseStart,
    /// Marks the completion of a pipeline phase.
    PhaseEnd,
    /// Marks the beginning of a tool or command execution.
    ToolStart,
    /// Marks the completion of a tool or command execution.
    ToolEnd,
    /// Error condition or failure event.
    Error,
    /// Success condition or completion event.
    Success,
    /// Informational message or status update.
    Info,
}

/// A structured log entry capturing task execution events and status updates.
///
/// Task log entries provide an audit trail of task progression through phases,
/// tool invocations, errors, and key decisions. Unlike raw build output
/// (`BuildLogEntry`), these are semantic, human-readable records designed for
/// task monitoring, debugging, and post-execution analysis.
///
/// Each entry is timestamped and associated with a specific pipeline phase,
/// allowing reconstruction of the task's execution timeline and identification
/// of where issues occurred.
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

/// Origin of a Task, tracking where it was created or imported from.
///
/// Task sources enable traceability back to issue trackers, pull requests,
/// or internal systems. This helps with linking task execution back to
/// external systems and maintaining audit trails.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskSource {
    /// Manually created by a user directly in Tundra.
    Manual,
    /// Imported from a GitHub issue.
    GithubIssue {
        /// The GitHub issue number.
        issue_number: u32,
    },
    /// Imported from a GitHub pull request.
    GithubPr {
        /// The GitHub PR number.
        pr_number: u32,
    },
    /// Imported from a GitLab issue.
    GitlabIssue {
        /// The GitLab issue IID (internal ID).
        iid: u32,
    },
    /// Imported from a Linear issue.
    LinearIssue {
        /// The Linear issue identifier (e.g., "ENG-123").
        identifier: String,
    },
    /// Imported from an external source or file.
    Import,
    /// Generated from the ideation pipeline.
    Ideation {
        /// The unique identifier of the originating idea.
        idea_id: String,
    },
}

// ---------------------------------------------------------------------------
// Task
// ---------------------------------------------------------------------------

/// A comprehensive execution context for a single work item in the Tundra pipeline.
///
/// Tasks represent the full lifecycle of work from discovery through completion,
/// progressing through defined phases (`TaskPhase`) and accumulating context,
/// logs, and artifacts along the way. Each task is linked to a parent `Bead`
/// and can contain multiple subtasks for granular execution tracking.
///
/// Tasks support stacked diffs (via `parent_task_id` and `stack_position`),
/// agent profiling for optimal model selection, and comprehensive logging
/// including both structured task logs and raw build output.
///
/// Key lifecycle milestones are tracked via timestamp fields (`created_at`,
/// `started_at`, `completed_at`) and progress is reflected through both the
/// current `phase` and a computed `progress_percent`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier for this task.
    pub id: Uuid,
    /// Human-readable task title (short description).
    pub title: String,
    /// Optional detailed description of the work to be done.
    pub description: Option<String>,
    /// Parent Bead ID linking this task to a higher-level work unit.
    pub bead_id: Uuid,
    /// Current pipeline phase (Discovery, Planning, Coding, QA, etc.).
    pub phase: TaskPhase,
    /// Approximate completion percentage (0-100), derived from phase.
    pub progress_percent: u8,
    /// Child subtasks for granular execution tracking.
    pub subtasks: Vec<Subtask>,
    /// Path to isolated git worktree for this task (if using worktree isolation).
    pub worktree_path: Option<String>,
    /// Git branch name where work is being performed.
    pub git_branch: Option<String>,
    /// Functional classification (Feature, BugFix, Refactoring, etc.).
    pub category: TaskCategory,
    /// Scheduling priority (Low, Medium, High, Urgent).
    pub priority: TaskPriority,
    /// Estimated effort and complexity (Trivial, Small, Medium, Large, Complex).
    pub complexity: TaskComplexity,
    /// Expected impact on the system or users (Low, Medium, High, Critical).
    pub impact: Option<TaskImpact>,
    /// Agent profile determining model and thinking level per phase.
    pub agent_profile: Option<AgentProfile>,
    /// Per-phase model and thinking configuration overrides.
    pub phase_configs: Vec<PhaseConfig>,
    /// When the task was created.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp.
    pub updated_at: DateTime<Utc>,
    /// When task execution began (first phase transition from Discovery).
    pub started_at: Option<DateTime<Utc>>,
    /// When the task reached Complete phase.
    pub completed_at: Option<DateTime<Utc>>,
    /// Error message if task failed or encountered an issue.
    pub error: Option<String>,
    /// Structured log entries tracking phase transitions and events.
    pub logs: Vec<TaskLogEntry>,
    /// QA report from the review phase (if QA has been performed).
    pub qa_report: Option<QaReport>,
    /// Origin of this task (Manual, GithubIssue, Import, Ideation, etc.).
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
    /// Captured build output lines (stdout/stderr) from pipeline execution.
    #[serde(default)]
    pub build_logs: Vec<BuildLogEntry>,
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
            build_logs: Vec::new(),
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

    /// Append a build output line captured from a pipeline command.
    pub fn add_build_log(&mut self, stream: BuildStream, line: impl Into<String>) {
        self.build_logs.push(BuildLogEntry {
            timestamp: Utc::now(),
            stream,
            line: line.into(),
            phase: self.phase.clone(),
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

/// Severity level of a QA issue, indicating its impact and urgency.
///
/// Used to prioritize issue resolution during the QA review phase.
/// Critical issues typically block merging, while minor issues may be
/// addressed in follow-up tasks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QaSeverity {
    /// Blocker issue requiring immediate attention before merge.
    Critical,
    /// Significant issue that should be addressed before release.
    Major,
    /// Low-impact issue that can be fixed in a follow-up.
    Minor,
}

/// Overall status of a QA review.
///
/// Determines whether a task can proceed to merging or requires fixes.
/// The status drives workflow transitions via `QaReport::next_phase()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QaStatus {
    /// All checks passed, ready to merge.
    Passed,
    /// One or more issues found, requires fixing.
    Failed,
    /// QA review in progress or not yet started.
    Pending,
}

/// A specific issue identified during QA review.
///
/// Captures details about problems found in code, tests, or documentation.
/// Issues are aggregated in a `QaReport` and can be linked to specific
/// file locations for easier resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaIssue {
    /// Unique identifier for this issue.
    pub id: Uuid,
    /// Severity level indicating impact and urgency.
    pub severity: QaSeverity,
    /// Human-readable description of the problem.
    pub description: String,
    /// Optional file path where the issue was found.
    pub file: Option<String>,
    /// Optional line number within the file.
    pub line: Option<u32>,
}

/// A comprehensive QA review report for a task.
///
/// Generated by QA reviewers to assess code quality, test coverage,
/// and adherence to standards. The report's status determines the
/// next workflow phase via `next_phase()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaReport {
    /// Unique identifier for this report.
    pub id: Uuid,
    /// ID of the task being reviewed.
    pub task_id: Uuid,
    /// Overall QA outcome (Passed, Failed, or Pending).
    pub status: QaStatus,
    /// List of issues found during review, if any.
    pub issues: Vec<QaIssue>,
    /// When this report was generated.
    pub timestamp: DateTime<Utc>,
}

impl QaReport {
    /// Create a new QA report for the given task with the specified status.
    ///
    /// Initializes the report with a new UUID, empty issues list, and
    /// current timestamp. Issues can be added after creation.
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
        self.issues
            .iter()
            .any(|i| i.severity == QaSeverity::Critical)
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

/// A file associated with a Task, tracking when and why it was added.
///
/// Files are categorized by type (Implementation, Test, Config, Documentation)
/// and linked to the specific phase and optionally subtask that introduced them.
/// This enables dependency tracking and change analysis across task execution.
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

/// Collection of files associated with a Task, with filtering helpers.
///
/// Provides efficient lookups and filtering by type, phase, and subtask to
/// support dependency analysis and change tracking during task execution.
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
        self.files
            .iter()
            .filter(|f| &f.file_type == file_type)
            .collect()
    }

    /// Filter files by phase.
    pub fn by_phase(&self, phase: &TaskPhase) -> Vec<&TaskFile> {
        self.files
            .iter()
            .filter(|f| &f.phase_added == phase)
            .collect()
    }

    /// Filter files by subtask.
    pub fn by_subtask(&self, subtask_id: Uuid) -> Vec<&TaskFile> {
        self.files
            .iter()
            .filter(|f| f.subtask_id == Some(subtask_id))
            .collect()
    }

    /// Count of files.
    pub fn count(&self) -> usize {
        self.files.len()
    }
}

// ---------------------------------------------------------------------------
// KpiSnapshot
// ---------------------------------------------------------------------------

/// Point-in-time snapshot of system metrics and workflow status.
///
/// Captures the distribution of beads across all lifecycle states and the
/// count of active agents, enabling performance monitoring, trend analysis,
/// and capacity planning. Used by the cache layer for historical metrics.
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
