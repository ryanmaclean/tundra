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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BeadStatus {
    Pending,
    InProgress,
    Review,
    Done,
    Failed,
}

impl std::fmt::Display for BeadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BeadStatus::Pending => write!(f, "Pending"),
            BeadStatus::InProgress => write!(f, "In Progress"),
            BeadStatus::Review => write!(f, "Review"),
            BeadStatus::Done => write!(f, "Done"),
            BeadStatus::Failed => write!(f, "Failed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Lane {
    Backlog,
    Hooked,
    Slung,
    Review,
    Done,
}

impl std::fmt::Display for Lane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Lane::Backlog => write!(f, "Backlog"),
            Lane::Hooked => write!(f, "Hooked"),
            Lane::Slung => write!(f, "Slung"),
            Lane::Review => write!(f, "Review"),
            Lane::Done => write!(f, "Done"),
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
        BeadResponse {
            id: "bead-001".into(),
            title: "Setup project scaffolding".into(),
            status: BeadStatus::Done,
            lane: Lane::Done,
            agent_id: Some("agent-001".into()),
            description: "Initialize workspace and crate structure".into(),
        },
        BeadResponse {
            id: "bead-002".into(),
            title: "Implement core types".into(),
            status: BeadStatus::Done,
            lane: Lane::Done,
            agent_id: Some("agent-002".into()),
            description: "Define bead, agent, and session types".into(),
        },
        BeadResponse {
            id: "bead-003".into(),
            title: "Build agent executor".into(),
            status: BeadStatus::InProgress,
            lane: Lane::Slung,
            agent_id: Some("agent-002".into()),
            description: "Agent lifecycle management".into(),
        },
        BeadResponse {
            id: "bead-004".into(),
            title: "MCP tool integration".into(),
            status: BeadStatus::InProgress,
            lane: Lane::Hooked,
            agent_id: Some("agent-003".into()),
            description: "Connect MCP servers to agents".into(),
        },
        BeadResponse {
            id: "bead-005".into(),
            title: "Review agent executor".into(),
            status: BeadStatus::Review,
            lane: Lane::Review,
            agent_id: Some("agent-004".into()),
            description: "Code review of executor module".into(),
        },
        BeadResponse {
            id: "bead-006".into(),
            title: "Telemetry pipeline".into(),
            status: BeadStatus::Pending,
            lane: Lane::Backlog,
            agent_id: None,
            description: "Set up cost and token tracking".into(),
        },
        BeadResponse {
            id: "bead-007".into(),
            title: "Session persistence".into(),
            status: BeadStatus::Pending,
            lane: Lane::Backlog,
            agent_id: None,
            description: "Persist session state to disk".into(),
        },
        BeadResponse {
            id: "bead-008".into(),
            title: "CLI commands".into(),
            status: BeadStatus::Pending,
            lane: Lane::Backlog,
            agent_id: None,
            description: "Implement at-cli subcommands".into(),
        },
    ]
}

pub fn demo_kpis() -> Vec<KpiResponse> {
    vec![
        KpiResponse { label: "Active Agents".into(), value: "3".into() },
        KpiResponse { label: "Total Beads".into(), value: "8".into() },
        KpiResponse { label: "Convoys".into(), value: "2".into() },
        KpiResponse { label: "Total Cost".into(), value: "$2.95".into() },
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
        CostEntry { agent_name: "Architect".into(), model: "claude-opus-4".into(), tokens: 45_230, cost_usd: 1.23 },
        CostEntry { agent_name: "Coder-A".into(), model: "claude-sonnet-4".into(), tokens: 128_400, cost_usd: 0.89 },
        CostEntry { agent_name: "Coder-B".into(), model: "claude-sonnet-4".into(), tokens: 67_100, cost_usd: 0.47 },
        CostEntry { agent_name: "Reviewer".into(), model: "claude-opus-4".into(), tokens: 12_050, cost_usd: 0.34 },
        CostEntry { agent_name: "Tester".into(), model: "claude-haiku-3".into(), tokens: 8_200, cost_usd: 0.02 },
    ]
}

pub fn demo_mcp_servers() -> Vec<McpServerEntry> {
    vec![
        McpServerEntry { name: "filesystem".into(), endpoint: "stdio://mcp-fs".into(), status: "connected".into(), tools_count: 12 },
        McpServerEntry { name: "git".into(), endpoint: "stdio://mcp-git".into(), status: "connected".into(), tools_count: 8 },
        McpServerEntry { name: "web-search".into(), endpoint: "http://localhost:3100".into(), status: "disconnected".into(), tools_count: 3 },
    ]
}
