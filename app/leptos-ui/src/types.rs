use serde::{Deserialize, Serialize};

// ── Agent ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentStatus {
    Active,
    Idle,
    Pending,
    Stopped,
    Unknown,
}

impl AgentStatus {
    pub fn glyph(&self) -> &'static str {
        match self {
            AgentStatus::Active => "@",
            AgentStatus::Idle => "*",
            AgentStatus::Pending => "!",
            AgentStatus::Stopped => "x",
            AgentStatus::Unknown => "?",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            AgentStatus::Active => "glyph-active",
            AgentStatus::Idle => "glyph-idle",
            AgentStatus::Pending => "glyph-pending",
            AgentStatus::Stopped => "glyph-stopped",
            AgentStatus::Unknown => "glyph-unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub id: String,
    pub name: String,
    pub role: String,
    pub model: String,
    pub status: AgentStatus,
    pub tokens_used: u64,
    pub cost_usd: f64,
}

// ── Bead ──

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BeadStatus {
    Planning,
    InProgress,
    AiReview,
    HumanReview,
    Done,
    Failed,
}

impl std::fmt::Display for BeadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BeadStatus::Planning => write!(f, "Planning"),
            BeadStatus::InProgress => write!(f, "In Progress"),
            BeadStatus::AiReview => write!(f, "AI Review"),
            BeadStatus::HumanReview => write!(f, "Human Review"),
            BeadStatus::Done => write!(f, "Done"),
            BeadStatus::Failed => write!(f, "Failed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Lane {
    Backlog,
    Queue,
    Planning,
    InProgress,
    AiReview,
    HumanReview,
    Done,
    PrCreated,
}

impl std::fmt::Display for Lane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Lane::Backlog => write!(f, "Backlog"),
            Lane::Queue => write!(f, "Queue"),
            Lane::Planning => write!(f, "Planning"),
            Lane::InProgress => write!(f, "In Progress"),
            Lane::AiReview => write!(f, "AI Review"),
            Lane::HumanReview => write!(f, "Human Review"),
            Lane::Done => write!(f, "Done"),
            Lane::PrCreated => write!(f, "PR Created"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeadResponse {
    pub id: String,
    pub title: String,
    pub status: BeadStatus,
    pub lane: Lane,
    pub agent_id: Option<String>,
    pub description: String,
    pub tags: Vec<String>,
    pub progress_stage: String,
    pub agent_names: Vec<String>,
    pub timestamp: String,
    pub action: Option<String>,
    /// Lightweight subtask status list for rendering progress dots.
    /// Each entry is one of: "pending", "in_progress", "complete", "failed".
    #[serde(default)]
    pub subtask_statuses: Vec<String>,
}

// ── KPI ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiResponse {
    pub label: String,
    pub value: String,
}

// ── Session ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResponse {
    pub id: String,
    pub name: String,
    pub started_at: String,
    pub agent_count: u32,
    pub bead_count: u32,
    pub status: String,
}

// ── Convoy ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvoyResponse {
    pub id: String,
    pub name: String,
    pub total_beads: u32,
    pub completed_beads: u32,
    pub status: String,
}

// ── Cost ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEntry {
    pub agent_name: String,
    pub model: String,
    pub tokens: u64,
    pub cost_usd: f64,
}

// ── Status ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub daemon_running: bool,
    pub active_agents: u32,
    pub total_beads: u32,
    pub uptime_secs: u64,
}

// ── MCP ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerEntry {
    pub name: String,
    pub endpoint: String,
    pub status: String,
    pub tools_count: u32,
}

// ── Ideation ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeaResponse {
    pub id: String,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub votes: u32,
}

// ── Roadmap ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoadmapPhase {
    pub name: String,
    pub date_range: String,
    pub status: String,
    pub features: Vec<String>,
}

// ── Context ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    pub path: String,
    pub description: String,
    pub last_modified: String,
}

// ── Worktrees ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeEntry {
    pub branch: String,
    pub path: String,
    pub status: String,
    pub last_commit: String,
}

// ── GitHub Issues ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubIssue {
    pub number: u32,
    pub title: String,
    pub labels: Vec<String>,
    pub assignee: Option<String>,
    pub state: String,
    pub created: String,
}

// ── GitHub PRs ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubPr {
    pub number: u32,
    pub title: String,
    pub author: String,
    pub status: String,
    pub reviewers: Vec<String>,
    pub created: String,
}

// ── Claude Sessions ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeSession {
    pub name: String,
    pub agent: String,
    pub model: String,
    pub duration: String,
    pub tokens: u64,
    pub status: String,
}

// ── Demo data constructors ──

pub fn demo_agents() -> Vec<AgentResponse> {
    vec![
        AgentResponse {
            id: "agent-001".into(),
            name: "Architect".into(),
            role: "lead".into(),
            model: "claude-opus-4".into(),
            status: AgentStatus::Active,
            tokens_used: 45_230,
            cost_usd: 1.23,
        },
        AgentResponse {
            id: "agent-002".into(),
            name: "Coder-A".into(),
            role: "implementer".into(),
            model: "claude-sonnet-4".into(),
            status: AgentStatus::Active,
            tokens_used: 128_400,
            cost_usd: 0.89,
        },
        AgentResponse {
            id: "agent-003".into(),
            name: "Coder-B".into(),
            role: "implementer".into(),
            model: "claude-sonnet-4".into(),
            status: AgentStatus::Idle,
            tokens_used: 67_100,
            cost_usd: 0.47,
        },
        AgentResponse {
            id: "agent-004".into(),
            name: "Reviewer".into(),
            role: "reviewer".into(),
            model: "claude-opus-4".into(),
            status: AgentStatus::Pending,
            tokens_used: 12_050,
            cost_usd: 0.34,
        },
        AgentResponse {
            id: "agent-005".into(),
            name: "Tester".into(),
            role: "tester".into(),
            model: "claude-haiku-3".into(),
            status: AgentStatus::Stopped,
            tokens_used: 8_200,
            cost_usd: 0.02,
        },
        AgentResponse {
            id: "agent-006".into(),
            name: "Doc-Writer".into(),
            role: "documenter".into(),
            model: "claude-haiku-3".into(),
            status: AgentStatus::Unknown,
            tokens_used: 0,
            cost_usd: 0.0,
        },
    ]
}

pub fn demo_beads() -> Vec<BeadResponse> {
    vec![
        // Backlog column (3 beads)
        BeadResponse {
            id: "bead-021".into(),
            title: "Multi-repo support".into(),
            status: BeadStatus::Planning,
            lane: Lane::Backlog,
            agent_id: None,
            description: "Support managing multiple git repositories in a single session".into(),
            tags: vec!["Feature".into()],
            progress_stage: "plan".into(),
            agent_names: vec![],
            timestamp: "3d ago".into(),
            action: Some("start".into()),
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-022".into(),
            title: "Agent memory system".into(),
            status: BeadStatus::Planning,
            lane: Lane::Backlog,
            agent_id: None,
            description: "Persistent memory across agent sessions using vector embeddings".into(),
            tags: vec!["Feature".into(), "High".into()],
            progress_stage: "plan".into(),
            agent_names: vec![],
            timestamp: "4d ago".into(),
            action: Some("start".into()),
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-023".into(),
            title: "Custom MCP tool builder".into(),
            status: BeadStatus::Planning,
            lane: Lane::Backlog,
            agent_id: None,
            description: "Visual tool for creating custom MCP server definitions".into(),
            tags: vec!["Feature".into()],
            progress_stage: "plan".into(),
            agent_names: vec![],
            timestamp: "5d ago".into(),
            action: Some("start".into()),
            subtask_statuses: vec![],
        },
        // Queue column (2 beads)
        BeadResponse {
            id: "bead-024".into(),
            title: "Cost budget alerts".into(),
            status: BeadStatus::Planning,
            lane: Lane::Queue,
            agent_id: None,
            description: "Send notifications when token spend approaches budget threshold".into(),
            tags: vec!["Feature".into()],
            progress_stage: "plan".into(),
            agent_names: vec![],
            timestamp: "1d ago".into(),
            action: Some("start".into()),
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-025".into(),
            title: "Agent health checks".into(),
            status: BeadStatus::Planning,
            lane: Lane::Queue,
            agent_id: None,
            description: "Periodic health monitoring for running agent processes".into(),
            tags: vec!["Feature".into(), "High".into()],
            progress_stage: "plan".into(),
            agent_names: vec![],
            timestamp: "2d ago".into(),
            action: Some("start".into()),
            subtask_statuses: vec![],
        },
        // Planning column (5 beads)
        BeadResponse {
            id: "bead-001".into(),
            title: "Design plugin architecture".into(),
            status: BeadStatus::Planning,
            lane: Lane::Planning,
            agent_id: None,
            description: "Define extensible plugin system for MCP tools".into(),
            tags: vec!["Feature".into(), "High".into()],
            progress_stage: "plan".into(),
            agent_names: vec![],
            timestamp: "1h ago".into(),
            action: Some("start".into()),
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-002".into(),
            title: "Session persistence layer".into(),
            status: BeadStatus::Planning,
            lane: Lane::Planning,
            agent_id: None,
            description: "Persist session state to SQLite".into(),
            tags: vec!["Feature".into()],
            progress_stage: "plan".into(),
            agent_names: vec![],
            timestamp: "3h ago".into(),
            action: Some("start".into()),
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-003".into(),
            title: "Rate limiting middleware".into(),
            status: BeadStatus::Planning,
            lane: Lane::Planning,
            agent_id: None,
            description: "Add token-based rate limiting per agent".into(),
            tags: vec!["Refactoring".into()],
            progress_stage: "plan".into(),
            agent_names: vec![],
            timestamp: "5h ago".into(),
            action: Some("start".into()),
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-004".into(),
            title: "Telemetry pipeline".into(),
            status: BeadStatus::Planning,
            lane: Lane::Planning,
            agent_id: None,
            description: "Set up cost and token tracking".into(),
            tags: vec!["Feature".into(), "High".into()],
            progress_stage: "plan".into(),
            agent_names: vec![],
            timestamp: "6h ago".into(),
            action: Some("start".into()),
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-005".into(),
            title: "CLI subcommands".into(),
            status: BeadStatus::Planning,
            lane: Lane::Planning,
            agent_id: None,
            description: "Implement at-cli start/stop/status commands".into(),
            tags: vec!["Feature".into()],
            progress_stage: "plan".into(),
            agent_names: vec![],
            timestamp: "8h ago".into(),
            action: Some("start".into()),
            subtask_statuses: vec![],
        },
        // In Progress column (4 beads)
        BeadResponse {
            id: "bead-006".into(),
            title: "Build agent executor".into(),
            status: BeadStatus::InProgress,
            lane: Lane::InProgress,
            agent_id: Some("agent-002".into()),
            description: "Agent lifecycle management and task dispatch".into(),
            tags: vec!["Feature".into(), "High".into()],
            progress_stage: "code".into(),
            agent_names: vec!["Coder-A".into()],
            timestamp: "5m ago".into(),
            action: None,
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-007".into(),
            title: "MCP tool integration".into(),
            status: BeadStatus::InProgress,
            lane: Lane::InProgress,
            agent_id: Some("agent-003".into()),
            description: "Connect MCP servers to agents via stdio".into(),
            tags: vec!["Feature".into()],
            progress_stage: "code".into(),
            agent_names: vec!["Coder-B".into()],
            timestamp: "12m ago".into(),
            action: None,
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-008".into(),
            title: "Worktree manager".into(),
            status: BeadStatus::InProgress,
            lane: Lane::InProgress,
            agent_id: Some("agent-002".into()),
            description: "Auto-create git worktrees per agent".into(),
            tags: vec!["Feature".into(), "Stuck".into()],
            progress_stage: "code".into(),
            agent_names: vec!["Coder-A".into(), "Architect".into()],
            timestamp: "25m ago".into(),
            action: Some("recover".into()),
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-009".into(),
            title: "WebSocket event stream".into(),
            status: BeadStatus::InProgress,
            lane: Lane::InProgress,
            agent_id: Some("agent-001".into()),
            description: "Real-time event streaming to UI clients".into(),
            tags: vec!["Feature".into()],
            progress_stage: "code".into(),
            agent_names: vec!["Architect".into()],
            timestamp: "30m ago".into(),
            action: None,
            subtask_statuses: vec![],
        },
        // AI Review column (3 beads)
        BeadResponse {
            id: "bead-010".into(),
            title: "Review agent executor v1".into(),
            status: BeadStatus::AiReview,
            lane: Lane::AiReview,
            agent_id: Some("agent-004".into()),
            description: "Automated code review of executor module".into(),
            tags: vec!["PR Created".into()],
            progress_stage: "qa".into(),
            agent_names: vec!["Reviewer".into()],
            timestamp: "2m ago".into(),
            action: None,
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-011".into(),
            title: "Validate config parser".into(),
            status: BeadStatus::AiReview,
            lane: Lane::AiReview,
            agent_id: Some("agent-004".into()),
            description: "Review TOML config parsing and validation".into(),
            tags: vec!["Refactoring".into()],
            progress_stage: "qa".into(),
            agent_names: vec!["Reviewer".into()],
            timestamp: "18m ago".into(),
            action: None,
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-012".into(),
            title: "Test harness scaffolding".into(),
            status: BeadStatus::AiReview,
            lane: Lane::AiReview,
            agent_id: Some("agent-005".into()),
            description: "Integration test framework setup review".into(),
            tags: vec!["Needs Recovery".into()],
            progress_stage: "qa".into(),
            agent_names: vec!["Tester".into()],
            timestamp: "45m ago".into(),
            action: Some("recover".into()),
            subtask_statuses: vec![],
        },
        // Human Review column (3 beads)
        BeadResponse {
            id: "bead-013".into(),
            title: "Core types refactor".into(),
            status: BeadStatus::HumanReview,
            lane: Lane::HumanReview,
            agent_id: Some("agent-001".into()),
            description: "Restructure bead/agent/session type hierarchy".into(),
            tags: vec!["Refactoring".into(), "PR Created".into()],
            progress_stage: "qa".into(),
            agent_names: vec!["Architect".into()],
            timestamp: "1h ago".into(),
            action: None,
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-014".into(),
            title: "Security audit: API auth".into(),
            status: BeadStatus::HumanReview,
            lane: Lane::HumanReview,
            agent_id: None,
            description: "Manual review of API authentication flow".into(),
            tags: vec!["High".into()],
            progress_stage: "qa".into(),
            agent_names: vec![],
            timestamp: "2h ago".into(),
            action: None,
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-015".into(),
            title: "Database migration plan".into(),
            status: BeadStatus::HumanReview,
            lane: Lane::HumanReview,
            agent_id: None,
            description: "Review migration strategy for session storage".into(),
            tags: vec!["Feature".into()],
            progress_stage: "qa".into(),
            agent_names: vec![],
            timestamp: "3h ago".into(),
            action: None,
            subtask_statuses: vec![],
        },
        // Done column (5 beads)
        BeadResponse {
            id: "bead-016".into(),
            title: "Setup project scaffolding".into(),
            status: BeadStatus::Done,
            lane: Lane::Done,
            agent_id: Some("agent-001".into()),
            description: "Initialize workspace and crate structure".into(),
            tags: vec!["Feature".into()],
            progress_stage: "done".into(),
            agent_names: vec!["Architect".into()],
            timestamp: "2d ago".into(),
            action: None,
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-017".into(),
            title: "Implement core types".into(),
            status: BeadStatus::Done,
            lane: Lane::Done,
            agent_id: Some("agent-002".into()),
            description: "Define bead, agent, and session types".into(),
            tags: vec!["Feature".into()],
            progress_stage: "done".into(),
            agent_names: vec!["Coder-A".into()],
            timestamp: "1d ago".into(),
            action: None,
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-018".into(),
            title: "Logger setup".into(),
            status: BeadStatus::Done,
            lane: Lane::Done,
            agent_id: Some("agent-003".into()),
            description: "Configure tracing and log output".into(),
            tags: vec!["Refactoring".into()],
            progress_stage: "done".into(),
            agent_names: vec!["Coder-B".into()],
            timestamp: "1d ago".into(),
            action: None,
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-019".into(),
            title: "Config file parser".into(),
            status: BeadStatus::Done,
            lane: Lane::Done,
            agent_id: Some("agent-002".into()),
            description: "Parse auto-tundra.toml configuration".into(),
            tags: vec!["Feature".into()],
            progress_stage: "done".into(),
            agent_names: vec!["Coder-A".into()],
            timestamp: "12h ago".into(),
            action: None,
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-020".into(),
            title: "Error handling framework".into(),
            status: BeadStatus::Failed,
            lane: Lane::Done,
            agent_id: Some("agent-003".into()),
            description: "Unified error types and recovery - agent crashed".into(),
            tags: vec!["Needs Recovery".into()],
            progress_stage: "done".into(),
            agent_names: vec!["Coder-B".into()],
            timestamp: "6h ago".into(),
            action: Some("resume".into()),
            subtask_statuses: vec![],
        },
        // PR Created column (2 beads)
        BeadResponse {
            id: "bead-026".into(),
            title: "Config hot-reload".into(),
            status: BeadStatus::Done,
            lane: Lane::PrCreated,
            agent_id: Some("agent-002".into()),
            description: "Watch config file for changes and reload without restart".into(),
            tags: vec!["Feature".into(), "PR Created".into()],
            progress_stage: "done".into(),
            agent_names: vec!["Coder-A".into()],
            timestamp: "4h ago".into(),
            action: None,
            subtask_statuses: vec![],
        },
        BeadResponse {
            id: "bead-027".into(),
            title: "Structured logging format".into(),
            status: BeadStatus::Done,
            lane: Lane::PrCreated,
            agent_id: Some("agent-003".into()),
            description: "JSON-structured log output with trace correlation IDs".into(),
            tags: vec!["Refactoring".into(), "PR Created".into()],
            progress_stage: "done".into(),
            agent_names: vec!["Coder-B".into()],
            timestamp: "8h ago".into(),
            action: None,
            subtask_statuses: vec![],
        },
    ]
}

pub fn demo_kpis() -> Vec<KpiResponse> {
    vec![
        KpiResponse {
            label: "Active Agents".into(),
            value: "3".into(),
        },
        KpiResponse {
            label: "Total Beads".into(),
            value: "20".into(),
        },
        KpiResponse {
            label: "Convoys".into(),
            value: "2".into(),
        },
        KpiResponse {
            label: "Total Cost".into(),
            value: "$2.95".into(),
        },
    ]
}

pub fn demo_sessions() -> Vec<SessionResponse> {
    vec![
        SessionResponse {
            id: "sess-001".into(),
            name: "scaffold-sprint".into(),
            started_at: "2025-01-15 09:30:00".into(),
            agent_count: 4,
            bead_count: 5,
            status: "active".into(),
        },
        SessionResponse {
            id: "sess-002".into(),
            name: "mcp-integration".into(),
            started_at: "2025-01-15 11:00:00".into(),
            agent_count: 2,
            bead_count: 3,
            status: "paused".into(),
        },
    ]
}

pub fn demo_convoys() -> Vec<ConvoyResponse> {
    vec![
        ConvoyResponse {
            id: "convoy-001".into(),
            name: "Core Implementation".into(),
            total_beads: 5,
            completed_beads: 2,
            status: "in_progress".into(),
        },
        ConvoyResponse {
            id: "convoy-002".into(),
            name: "Integration Layer".into(),
            total_beads: 3,
            completed_beads: 0,
            status: "pending".into(),
        },
    ]
}

pub fn demo_costs() -> Vec<CostEntry> {
    vec![
        CostEntry {
            agent_name: "Architect".into(),
            model: "claude-opus-4".into(),
            tokens: 45_230,
            cost_usd: 1.23,
        },
        CostEntry {
            agent_name: "Coder-A".into(),
            model: "claude-sonnet-4".into(),
            tokens: 128_400,
            cost_usd: 0.89,
        },
        CostEntry {
            agent_name: "Coder-B".into(),
            model: "claude-sonnet-4".into(),
            tokens: 67_100,
            cost_usd: 0.47,
        },
        CostEntry {
            agent_name: "Reviewer".into(),
            model: "claude-opus-4".into(),
            tokens: 12_050,
            cost_usd: 0.34,
        },
        CostEntry {
            agent_name: "Tester".into(),
            model: "claude-haiku-3".into(),
            tokens: 8_200,
            cost_usd: 0.02,
        },
    ]
}

pub fn demo_mcp_servers() -> Vec<McpServerEntry> {
    vec![
        McpServerEntry {
            name: "filesystem".into(),
            endpoint: "stdio://mcp-fs".into(),
            status: "connected".into(),
            tools_count: 12,
        },
        McpServerEntry {
            name: "git".into(),
            endpoint: "stdio://mcp-git".into(),
            status: "connected".into(),
            tools_count: 8,
        },
        McpServerEntry {
            name: "web-search".into(),
            endpoint: "http://localhost:3100".into(),
            status: "disconnected".into(),
            tools_count: 3,
        },
    ]
}

pub fn demo_ideas() -> Vec<IdeaResponse> {
    vec![
        IdeaResponse {
            id: "idea-001".into(),
            title: "Auto-retry failed beads".into(),
            description: "Automatically retry beads that fail due to transient errors with exponential backoff".into(),
            tags: vec!["resilience".into(), "automation".into()],
            votes: 12,
        },
        IdeaResponse {
            id: "idea-002".into(),
            title: "Agent skill specialization".into(),
            description: "Let agents declare skill profiles so tasks are routed to the best-fit agent".into(),
            tags: vec!["agents".into(), "optimization".into()],
            votes: 8,
        },
        IdeaResponse {
            id: "idea-003".into(),
            title: "Cost budget alerts".into(),
            description: "Send notifications when token spend approaches a configurable budget threshold".into(),
            tags: vec!["cost".into(), "monitoring".into()],
            votes: 15,
        },
        IdeaResponse {
            id: "idea-004".into(),
            title: "Multi-repo worktree support".into(),
            description: "Extend worktree manager to handle multiple repositories in a single session".into(),
            tags: vec!["git".into(), "feature".into()],
            votes: 6,
        },
        IdeaResponse {
            id: "idea-005".into(),
            title: "Interactive bead debugger".into(),
            description: "Step through bead execution with breakpoints and state inspection".into(),
            tags: vec!["debugging".into(), "dx".into()],
            votes: 10,
        },
    ]
}

pub fn demo_roadmap() -> Vec<RoadmapPhase> {
    vec![
        RoadmapPhase {
            name: "Core Infrastructure".into(),
            date_range: "Jan 2025 - Feb 2025".into(),
            status: "completed".into(),
            features: vec![
                "Project scaffolding and workspace setup".into(),
                "Core types (Bead, Agent, Session)".into(),
                "Configuration parser (TOML)".into(),
                "Logging and tracing framework".into(),
                "Basic CLI commands".into(),
            ],
        },
        RoadmapPhase {
            name: "UI & Integration".into(),
            date_range: "Mar 2025 - Apr 2025".into(),
            status: "active".into(),
            features: vec![
                "Leptos WASM frontend".into(),
                "MCP server integration".into(),
                "Agent executor and lifecycle".into(),
                "WebSocket real-time events".into(),
                "Git worktree management".into(),
            ],
        },
        RoadmapPhase {
            name: "Advanced Features".into(),
            date_range: "May 2025 - Jul 2025".into(),
            status: "planned".into(),
            features: vec![
                "Multi-agent convoy orchestration".into(),
                "Cost optimization and budgeting".into(),
                "Plugin architecture".into(),
                "Session replay and debugging".into(),
                "GitHub integration (issues, PRs)".into(),
            ],
        },
    ]
}

pub fn demo_context_entries() -> Vec<ContextEntry> {
    vec![
        ContextEntry {
            path: "CLAUDE.md".into(),
            description: "Project-level instructions for Claude Code agents".into(),
            last_modified: "2h ago".into(),
        },
        ContextEntry {
            path: ".cursorrules".into(),
            description: "Cursor IDE rules and code style guidelines".into(),
            last_modified: "1d ago".into(),
        },
        ContextEntry {
            path: "docs/arch.md".into(),
            description: "System architecture overview and component diagram".into(),
            last_modified: "3d ago".into(),
        },
        ContextEntry {
            path: "docs/api.md".into(),
            description: "REST and WebSocket API specification".into(),
            last_modified: "5d ago".into(),
        },
        ContextEntry {
            path: ".auto-tundra/config.toml".into(),
            description: "Daemon and agent configuration".into(),
            last_modified: "12h ago".into(),
        },
        ContextEntry {
            path: "AGENTS.md".into(),
            description: "Agent role definitions and capabilities".into(),
            last_modified: "2d ago".into(),
        },
    ]
}

pub fn demo_worktrees() -> Vec<WorktreeEntry> {
    vec![
        WorktreeEntry {
            branch: "main".into(),
            path: "/home/dev/auto-tundra".into(),
            status: "active".into(),
            last_commit: "fix: resolve config parsing edge case".into(),
        },
        WorktreeEntry {
            branch: "feat/agent-executor".into(),
            path: "/home/dev/auto-tundra-wt/agent-executor".into(),
            status: "active".into(),
            last_commit: "wip: agent lifecycle state machine".into(),
        },
        WorktreeEntry {
            branch: "feat/mcp-integration".into(),
            path: "/home/dev/auto-tundra-wt/mcp-integration".into(),
            status: "active".into(),
            last_commit: "feat: add stdio transport for MCP".into(),
        },
        WorktreeEntry {
            branch: "fix/websocket-reconnect".into(),
            path: "/home/dev/auto-tundra-wt/ws-reconnect".into(),
            status: "stale".into(),
            last_commit: "chore: stash reconnect logic".into(),
        },
    ]
}

pub fn demo_github_issues() -> Vec<GithubIssue> {
    vec![
        GithubIssue {
            number: 42,
            title: "Agent crashes on malformed MCP response".into(),
            labels: vec!["bug".into(), "critical".into()],
            assignee: Some("coder-a".into()),
            state: "open".into(),
            created: "2025-02-10".into(),
        },
        GithubIssue {
            number: 38,
            title: "Add support for HTTP MCP transport".into(),
            labels: vec!["enhancement".into(), "mcp".into()],
            assignee: Some("coder-b".into()),
            state: "open".into(),
            created: "2025-02-08".into(),
        },
        GithubIssue {
            number: 35,
            title: "Config hot-reload not working on Linux".into(),
            labels: vec!["bug".into(), "linux".into()],
            assignee: None,
            state: "open".into(),
            created: "2025-02-05".into(),
        },
        GithubIssue {
            number: 31,
            title: "Document convoy orchestration API".into(),
            labels: vec!["docs".into()],
            assignee: Some("doc-writer".into()),
            state: "open".into(),
            created: "2025-01-30".into(),
        },
        GithubIssue {
            number: 27,
            title: "Implement token budget per session".into(),
            labels: vec!["enhancement".into(), "cost".into()],
            assignee: None,
            state: "open".into(),
            created: "2025-01-25".into(),
        },
        GithubIssue {
            number: 22,
            title: "Setup CI pipeline with cargo test".into(),
            labels: vec!["infra".into()],
            assignee: Some("architect".into()),
            state: "closed".into(),
            created: "2025-01-18".into(),
        },
    ]
}

pub fn demo_github_prs() -> Vec<GithubPr> {
    vec![
        GithubPr {
            number: 41,
            title: "feat: agent executor v1".into(),
            author: "coder-a".into(),
            status: "open".into(),
            reviewers: vec!["reviewer".into(), "architect".into()],
            created: "2025-02-12".into(),
        },
        GithubPr {
            number: 39,
            title: "refactor: core types hierarchy".into(),
            author: "architect".into(),
            status: "open".into(),
            reviewers: vec!["coder-a".into()],
            created: "2025-02-09".into(),
        },
        GithubPr {
            number: 36,
            title: "feat: MCP stdio transport".into(),
            author: "coder-b".into(),
            status: "draft".into(),
            reviewers: vec![],
            created: "2025-02-06".into(),
        },
        GithubPr {
            number: 33,
            title: "fix: config parser edge cases".into(),
            author: "coder-a".into(),
            status: "merged".into(),
            reviewers: vec!["reviewer".into()],
            created: "2025-02-01".into(),
        },
        GithubPr {
            number: 28,
            title: "feat: tracing and structured logging".into(),
            author: "coder-b".into(),
            status: "merged".into(),
            reviewers: vec!["architect".into()],
            created: "2025-01-26".into(),
        },
    ]
}

pub fn demo_claude_sessions() -> Vec<ClaudeSession> {
    vec![
        ClaudeSession {
            name: "agent-executor-impl".into(),
            agent: "Coder-A".into(),
            model: "claude-sonnet-4".into(),
            duration: "45m".into(),
            tokens: 84_200,
            status: "active".into(),
        },
        ClaudeSession {
            name: "mcp-integration".into(),
            agent: "Coder-B".into(),
            model: "claude-sonnet-4".into(),
            duration: "32m".into(),
            tokens: 62_400,
            status: "active".into(),
        },
        ClaudeSession {
            name: "architecture-review".into(),
            agent: "Architect".into(),
            model: "claude-opus-4".into(),
            duration: "1h 12m".into(),
            tokens: 145_800,
            status: "idle".into(),
        },
    ]
}
