use chrono::{DateTime, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use at_core::types::{
    AgentRole, AgentStatus, BeadStatus, CliType, ConvoyStatus, Lane,
};

/// Tab names displayed in the header.
pub const TAB_NAMES: &[&str] = &[
    "Dashboard",
    "Agents",
    "Beads",
    "Sessions",
    "Convoys",
    "Costs",
    "Analytics",
    "Config",
    "MCP",
];

// ---------------------------------------------------------------------------
// Demo data structs (lightweight projections for the TUI)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub name: String,
    pub role: AgentRole,
    pub cli_type: CliType,
    pub model: String,
    pub status: AgentStatus,
    pub last_seen: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct BeadInfo {
    pub id: String,
    pub title: String,
    pub status: BeadStatus,
    pub lane: Lane,
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub agent: String,
    pub cli_type: CliType,
    pub status: String,
    pub duration: String,
    pub cpu: String,
}

#[derive(Debug, Clone)]
pub struct ConvoyInfo {
    pub name: String,
    pub status: ConvoyStatus,
    pub bead_count: usize,
    pub progress: u16,
}

#[derive(Debug, Clone)]
pub struct CostRow {
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub transport: String,
    pub status: String,
    pub tools: u32,
}

#[derive(Debug, Clone)]
pub struct ActivityEntry {
    pub timestamp: DateTime<Utc>,
    pub message: String,
}

// ---------------------------------------------------------------------------
// KPI snapshot for dashboard
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct KpiView {
    pub active_agents: u64,
    pub total_beads: u64,
    pub active_convoys: u64,
    pub total_cost: f64,
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

pub struct App {
    pub current_tab: usize,
    pub should_quit: bool,
    pub show_help: bool,

    /// Per-tab selected index for list navigation.
    pub selected_index: usize,
    /// Kanban column cursor (for Beads tab).
    pub kanban_column: usize,

    // Data
    pub agents: Vec<AgentInfo>,
    pub beads: Vec<BeadInfo>,
    pub sessions: Vec<SessionInfo>,
    pub convoys: Vec<ConvoyInfo>,
    pub costs: Vec<CostRow>,
    pub mcp_servers: Vec<McpServerInfo>,
    pub activity: Vec<ActivityEntry>,
    pub kpi: KpiView,
    pub config_text: String,
}

impl App {
    pub fn new() -> Self {
        Self {
            current_tab: 0,
            should_quit: false,
            show_help: false,
            selected_index: 0,
            kanban_column: 0,
            agents: demo_agents(),
            beads: demo_beads(),
            sessions: demo_sessions(),
            convoys: demo_convoys(),
            costs: demo_costs(),
            mcp_servers: demo_mcp(),
            activity: demo_activity(),
            kpi: demo_kpi(),
            config_text: load_config_text(),
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        // Help modal intercepts Esc and ?
        if self.show_help {
            match key.code {
                KeyCode::Char('?') | KeyCode::Esc => self.show_help = false,
                _ => {}
            }
            return;
        }

        match key.code {
            // Quit
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }

            // Tab switching: 1-9
            KeyCode::Char(c @ '1'..='9') => {
                let idx = (c as usize) - ('1' as usize);
                if idx < TAB_NAMES.len() {
                    self.current_tab = idx;
                    self.selected_index = 0;
                }
            }

            // Tab / Shift-Tab
            KeyCode::Tab => {
                self.current_tab = (self.current_tab + 1) % TAB_NAMES.len();
                self.selected_index = 0;
            }
            KeyCode::BackTab => {
                self.current_tab = if self.current_tab == 0 {
                    TAB_NAMES.len() - 1
                } else {
                    self.current_tab - 1
                };
                self.selected_index = 0;
            }

            // List navigation
            KeyCode::Char('j') | KeyCode::Down => {
                let max = self.current_list_len();
                if max > 0 && self.selected_index < max - 1 {
                    self.selected_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }

            // Kanban left/right
            KeyCode::Char('h') | KeyCode::Left => {
                if self.kanban_column > 0 {
                    self.kanban_column -= 1;
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if self.kanban_column < 4 {
                    self.kanban_column += 1;
                }
            }

            // Help
            KeyCode::Char('?') => self.show_help = true,

            // Refresh (reload demo data)
            KeyCode::Char('r') => {
                self.config_text = load_config_text();
            }

            _ => {}
        }
    }

    /// Returns the length of the primary list for the current tab.
    fn current_list_len(&self) -> usize {
        match self.current_tab {
            0 => self.agents.len(),  // dashboard agent panel
            1 => self.agents.len(),
            2 => self.beads.len(),
            3 => self.sessions.len(),
            4 => self.convoys.len(),
            5 => self.costs.len(),
            6 => 0,
            7 => 0,
            8 => self.mcp_servers.len(),
            _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Status glyph helper
// ---------------------------------------------------------------------------

pub fn status_glyph(status: &str) -> &'static str {
    match status {
        "active" => "@",
        "idle" => "*",
        "pending" => "!",
        "unknown" => "?",
        "stopped" => "x",
        _ => "-",
    }
}

// ---------------------------------------------------------------------------
// Demo data factories
// ---------------------------------------------------------------------------

fn demo_agents() -> Vec<AgentInfo> {
    let now = Utc::now();
    vec![
        AgentInfo {
            name: "mayor-alpha".into(),
            role: AgentRole::Mayor,
            cli_type: CliType::Claude,
            model: "claude-opus-4".into(),
            status: AgentStatus::Active,
            last_seen: now,
        },
        AgentInfo {
            name: "deacon-bravo".into(),
            role: AgentRole::Deacon,
            cli_type: CliType::Claude,
            model: "claude-sonnet-4".into(),
            status: AgentStatus::Active,
            last_seen: now,
        },
        AgentInfo {
            name: "crew-charlie".into(),
            role: AgentRole::Crew,
            cli_type: CliType::Codex,
            model: "o3".into(),
            status: AgentStatus::Idle,
            last_seen: now,
        },
        AgentInfo {
            name: "crew-delta".into(),
            role: AgentRole::Crew,
            cli_type: CliType::Gemini,
            model: "gemini-2.5-pro".into(),
            status: AgentStatus::Pending,
            last_seen: now,
        },
        AgentInfo {
            name: "witness-echo".into(),
            role: AgentRole::Witness,
            cli_type: CliType::Claude,
            model: "claude-sonnet-4".into(),
            status: AgentStatus::Stopped,
            last_seen: now,
        },
    ]
}

fn demo_beads() -> Vec<BeadInfo> {
    vec![
        BeadInfo { id: "bd-001".into(), title: "Set up CI pipeline".into(), status: BeadStatus::Done, lane: Lane::Standard },
        BeadInfo { id: "bd-002".into(), title: "Implement auth module".into(), status: BeadStatus::Slung, lane: Lane::Critical },
        BeadInfo { id: "bd-003".into(), title: "Add unit tests for core".into(), status: BeadStatus::Slung, lane: Lane::Standard },
        BeadInfo { id: "bd-004".into(), title: "Design TUI layout".into(), status: BeadStatus::Review, lane: Lane::Standard },
        BeadInfo { id: "bd-005".into(), title: "Write API docs".into(), status: BeadStatus::Hooked, lane: Lane::Experimental },
        BeadInfo { id: "bd-006".into(), title: "Refactor config loader".into(), status: BeadStatus::Backlog, lane: Lane::Standard },
        BeadInfo { id: "bd-007".into(), title: "Add MCP transport".into(), status: BeadStatus::Backlog, lane: Lane::Critical },
        BeadInfo { id: "bd-008".into(), title: "Optimize token usage".into(), status: BeadStatus::Backlog, lane: Lane::Experimental },
        BeadInfo { id: "bd-009".into(), title: "Setup monitoring".into(), status: BeadStatus::Done, lane: Lane::Standard },
        BeadInfo { id: "bd-010".into(), title: "Convoy orchestration".into(), status: BeadStatus::Hooked, lane: Lane::Critical },
    ]
}

fn demo_sessions() -> Vec<SessionInfo> {
    vec![
        SessionInfo { id: "sess-01".into(), agent: "mayor-alpha".into(), cli_type: CliType::Claude, status: "running".into(), duration: "12m 34s".into(), cpu: "2.1%".into() },
        SessionInfo { id: "sess-02".into(), agent: "deacon-bravo".into(), cli_type: CliType::Claude, status: "running".into(), duration: "8m 12s".into(), cpu: "1.4%".into() },
        SessionInfo { id: "sess-03".into(), agent: "crew-charlie".into(), cli_type: CliType::Codex, status: "idle".into(), duration: "45m 01s".into(), cpu: "0.0%".into() },
        SessionInfo { id: "sess-04".into(), agent: "crew-delta".into(), cli_type: CliType::Gemini, status: "starting".into(), duration: "0m 03s".into(), cpu: "0.5%".into() },
    ]
}

fn demo_convoys() -> Vec<ConvoyInfo> {
    vec![
        ConvoyInfo { name: "auth-feature".into(), status: ConvoyStatus::Active, bead_count: 3, progress: 66 },
        ConvoyInfo { name: "ci-setup".into(), status: ConvoyStatus::Completed, bead_count: 2, progress: 100 },
        ConvoyInfo { name: "api-docs".into(), status: ConvoyStatus::Forming, bead_count: 4, progress: 10 },
    ]
}

fn demo_costs() -> Vec<CostRow> {
    vec![
        CostRow { provider: "Anthropic".into(), model: "claude-opus-4".into(), input_tokens: 125_000, output_tokens: 42_000, cost_usd: 3.45 },
        CostRow { provider: "Anthropic".into(), model: "claude-sonnet-4".into(), input_tokens: 310_000, output_tokens: 98_000, cost_usd: 2.12 },
        CostRow { provider: "OpenAI".into(), model: "o3".into(), input_tokens: 80_000, output_tokens: 25_000, cost_usd: 1.80 },
        CostRow { provider: "Google".into(), model: "gemini-2.5-pro".into(), input_tokens: 50_000, output_tokens: 15_000, cost_usd: 0.65 },
    ]
}

fn demo_mcp() -> Vec<McpServerInfo> {
    vec![
        McpServerInfo { name: "filesystem".into(), transport: "stdio".into(), status: "connected".into(), tools: 8 },
        McpServerInfo { name: "git".into(), transport: "stdio".into(), status: "connected".into(), tools: 12 },
        McpServerInfo { name: "postgres".into(), transport: "sse".into(), status: "disconnected".into(), tools: 5 },
        McpServerInfo { name: "web-search".into(), transport: "stdio".into(), status: "connected".into(), tools: 3 },
    ]
}

fn demo_activity() -> Vec<ActivityEntry> {
    let now = Utc::now();
    vec![
        ActivityEntry { timestamp: now, message: "mayor-alpha hooked bead bd-002".into() },
        ActivityEntry { timestamp: now, message: "deacon-bravo completed review on bd-004".into() },
        ActivityEntry { timestamp: now, message: "crew-charlie went idle".into() },
        ActivityEntry { timestamp: now, message: "convoy auth-feature progress: 66%".into() },
        ActivityEntry { timestamp: now, message: "crew-delta spawned with gemini-2.5-pro".into() },
    ]
}

fn demo_kpi() -> KpiView {
    KpiView {
        active_agents: 2,
        total_beads: 10,
        active_convoys: 1,
        total_cost: 8.02,
    }
}

fn load_config_text() -> String {
    let path = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".auto-tundra")
        .join("config.toml");
    if path.exists() {
        std::fs::read_to_string(&path).unwrap_or_else(|_| "(error reading config)".into())
    } else {
        // Show default config
        at_core::config::Config::default()
            .to_toml()
            .unwrap_or_else(|_| "(error serializing default config)".into())
    }
}
