use chrono::{DateTime, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use at_core::types::{AgentRole, AgentStatus, BeadStatus, CliType, ConvoyStatus, Lane};

use crate::api_client;

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
    "Roadmap",
    "Ideation",
    "Planning Poker",
    "Worktrees",
    "GitHub Issues",
    "GitHub PRs",
    "Stacks",
    "Context",
    "Changelog",
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

#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub id: String,
    pub path: String,
    pub branch: String,
    pub bead_id: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct GithubIssueInfo {
    pub number: u32,
    pub title: String,
    pub labels: Vec<String>,
    pub assignee: Option<String>,
    pub state: String,
    pub created: String,
}

#[derive(Debug, Clone)]
pub struct GithubPrInfo {
    pub number: u32,
    pub title: String,
    pub author: String,
    pub status: String,
    pub reviewers: Vec<String>,
    pub created: String,
}

#[derive(Debug, Clone)]
pub struct RoadmapItemInfo {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
}

#[derive(Debug, Clone)]
pub struct IdeaInfo {
    pub id: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub impact: String,
    pub effort: String,
}

#[derive(Debug, Clone)]
pub struct StackNodeInfo {
    pub id: String,
    pub title: String,
    pub phase: String,
    pub git_branch: Option<String>,
    pub pr_number: Option<u32>,
    pub depth: usize,
}

#[derive(Debug, Clone)]
pub struct ChangelogEntryInfo {
    pub version: String,
    pub date: String,
    pub sections: Vec<(String, Vec<String>)>,
    pub expanded: bool,
}

#[derive(Debug, Clone)]
pub struct MemoryEntryInfo {
    pub id: String,
    pub category: String,
    pub content: String,
    pub created_at: String,
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
    /// Sub-tab index (for Context tab: 0=Index, 1=Memory).
    pub context_sub_tab: usize,

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
    pub effects: crate::effects::EffectManager,
    pub last_tick: std::time::Instant,

    // New data for additional tabs
    pub worktrees: Vec<WorktreeInfo>,
    pub github_issues: Vec<GithubIssueInfo>,
    pub github_prs: Vec<GithubPrInfo>,
    pub roadmap_items: Vec<RoadmapItemInfo>,
    pub ideas: Vec<IdeaInfo>,
    pub stacks: Vec<StackNodeInfo>,
    pub changelog: Vec<ChangelogEntryInfo>,
    pub memory_entries: Vec<MemoryEntryInfo>,

    /// Whether we're running in offline (demo data) mode.
    pub offline: bool,
    /// Connection status indicator.
    pub api_connected: bool,

    // Command mode
    pub in_command_mode: bool,
    pub command_buffer: String,
    pub command_result: Option<String>,

    // Toast notifications
    pub toasts: crate::widgets::toast::ToastManager,
}

impl App {
    pub fn new(offline: bool) -> Self {
        let mut effects = crate::effects::EffectManager::new();
        effects.add(crate::effects::fade_in());
        Self {
            current_tab: 0,
            should_quit: false,
            show_help: false,
            selected_index: 0,
            kanban_column: 0,
            context_sub_tab: 0,
            agents: demo_agents(),
            beads: demo_beads(),
            sessions: demo_sessions(),
            convoys: demo_convoys(),
            costs: demo_costs(),
            mcp_servers: demo_mcp(),
            activity: demo_activity(),
            kpi: demo_kpi(),
            config_text: load_config_text(),
            effects,
            last_tick: std::time::Instant::now(),
            worktrees: demo_worktrees(),
            github_issues: demo_github_issues(),
            github_prs: demo_github_prs(),
            roadmap_items: demo_roadmap(),
            ideas: demo_ideas(),
            stacks: demo_stacks(),
            changelog: demo_changelog(),
            memory_entries: demo_memory(),
            offline,
            api_connected: false,
            in_command_mode: false,
            command_buffer: String::new(),
            command_result: None,
            toasts: crate::widgets::toast::ToastManager::new(),
        }
    }

    /// Apply a snapshot of data fetched from the API.
    pub fn apply_data(&mut self, data: api_client::AppData) {
        self.api_connected = true;

        // Agents
        let now = Utc::now();
        self.agents = data
            .agents
            .into_iter()
            .map(|a| AgentInfo {
                name: a.name.clone(),
                role: parse_role(&a.role),
                cli_type: CliType::Claude,
                model: String::new(),
                status: parse_agent_status(&a.status),
                last_seen: now,
            })
            .collect();

        // Beads
        self.beads = data
            .beads
            .into_iter()
            .map(|b| BeadInfo {
                id: b.id,
                title: b.title,
                status: parse_bead_status(&b.status),
                lane: parse_lane(&b.lane),
            })
            .collect();

        // KPI
        self.kpi = KpiView {
            active_agents: data.kpi.active_agents,
            total_beads: data.kpi.total_beads,
            active_convoys: self
                .convoys
                .iter()
                .filter(|c| matches!(c.status, ConvoyStatus::Active))
                .count() as u64,
            total_cost: 0.0,
        };

        // Sessions
        self.sessions = data
            .sessions
            .into_iter()
            .map(|s| SessionInfo {
                id: s.id,
                agent: s.agent_name,
                cli_type: parse_cli_type(&s.cli_type),
                status: s.status,
                duration: s.duration,
                cpu: String::new(),
            })
            .collect();

        // Convoys
        self.convoys = data
            .convoys
            .into_iter()
            .map(|c| ConvoyInfo {
                name: c.name,
                status: parse_convoy_status(&c.status),
                bead_count: c.bead_count as usize,
                progress: 0,
            })
            .collect();

        // Update KPI active convoys after convoys are set
        self.kpi.active_convoys = self
            .convoys
            .iter()
            .filter(|c| matches!(c.status, ConvoyStatus::Active))
            .count() as u64;

        // Costs
        self.costs = data
            .costs
            .sessions
            .into_iter()
            .map(|s| CostRow {
                provider: String::new(),
                model: String::new(),
                input_tokens: s.input_tokens,
                output_tokens: s.output_tokens,
                cost_usd: 0.0,
            })
            .collect();

        // MCP
        self.mcp_servers = data
            .mcp_servers
            .into_iter()
            .map(|m| McpServerInfo {
                name: m.name,
                transport: "stdio".into(),
                status: m.status,
                tools: m.tools.len() as u32,
            })
            .collect();

        // Worktrees
        self.worktrees = data
            .worktrees
            .into_iter()
            .map(|w| WorktreeInfo {
                id: w.id,
                path: w.path,
                branch: w.branch,
                bead_id: w.bead_id,
                status: w.status,
            })
            .collect();

        // GitHub Issues
        self.github_issues = data
            .github_issues
            .into_iter()
            .map(|i| GithubIssueInfo {
                number: i.number,
                title: i.title,
                labels: i.labels,
                assignee: i.assignee,
                state: i.state,
                created: i.created,
            })
            .collect();

        // GitHub PRs
        self.github_prs = data
            .github_prs
            .into_iter()
            .map(|p| GithubPrInfo {
                number: p.number,
                title: p.title,
                author: p.author,
                status: p.status,
                reviewers: p.reviewers,
                created: p.created,
            })
            .collect();

        // Roadmap
        self.roadmap_items = data
            .roadmap_items
            .into_iter()
            .map(|r| RoadmapItemInfo {
                id: r.id,
                title: r.title,
                description: r.description,
                status: r.status,
                priority: r.priority,
            })
            .collect();

        // Ideas
        self.ideas = data
            .ideas
            .into_iter()
            .map(|i| IdeaInfo {
                id: i.id,
                title: i.title,
                description: i.description,
                category: i.category,
                impact: i.impact,
                effort: i.effort,
            })
            .collect();

        // Stacks
        self.stacks = data
            .stacks
            .into_iter()
            .flat_map(|s| {
                let mut nodes = vec![StackNodeInfo {
                    id: s.root.id,
                    title: s.root.title,
                    phase: s.root.phase,
                    git_branch: s.root.git_branch,
                    pr_number: s.root.pr_number,
                    depth: 0,
                }];
                for child in s.children {
                    nodes.push(StackNodeInfo {
                        id: child.id,
                        title: child.title,
                        phase: child.phase,
                        git_branch: child.git_branch,
                        pr_number: child.pr_number,
                        depth: 1,
                    });
                }
                nodes
            })
            .collect();

        // Changelog
        self.changelog = data
            .changelog
            .into_iter()
            .map(|c| ChangelogEntryInfo {
                version: c.version,
                date: c.date,
                sections: c
                    .sections
                    .into_iter()
                    .map(|s| (s.category, s.items))
                    .collect(),
                expanded: false,
            })
            .collect();

        // Memory
        self.memory_entries = data
            .memory
            .into_iter()
            .map(|m| MemoryEntryInfo {
                id: m.id,
                category: m.category,
                content: m.content,
                created_at: m.created_at,
            })
            .collect();
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        // Command mode intercepts all input
        if self.in_command_mode {
            match key.code {
                KeyCode::Esc => {
                    self.in_command_mode = false;
                    self.command_buffer.clear();
                }
                KeyCode::Enter => {
                    let input = format!(":{}", self.command_buffer);
                    self.in_command_mode = false;
                    self.command_buffer.clear();
                    if let Some(cmd) = crate::command::parse_command(&input) {
                        self.command_result = crate::command::execute_command(self, cmd);
                    }
                }
                KeyCode::Backspace => {
                    self.command_buffer.pop();
                }
                KeyCode::Char(c) => {
                    self.command_buffer.push(c);
                }
                _ => {}
            }
            return;
        }

        // Help modal intercepts Esc and ?
        if self.show_help {
            match key.code {
                KeyCode::Char('?') | KeyCode::Esc => self.show_help = false,
                _ => {}
            }
            return;
        }

        match key.code {
            // Enter command mode
            KeyCode::Char(':') => {
                self.in_command_mode = true;
                self.command_buffer.clear();
                self.command_result = None;
            }
            // Quit
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }

            // Tab switching: 1-9, 0 for tab 10
            KeyCode::Char(c @ '1'..='9') => {
                let idx = (c as usize) - ('1' as usize);
                if idx < TAB_NAMES.len() {
                    self.current_tab = idx;
                    self.selected_index = 0;
                }
            }
            KeyCode::Char('0') => {
                if 9 < TAB_NAMES.len() {
                    self.current_tab = 9;
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

            // Quick-jump letter shortcuts for tabs >9
            KeyCode::Char('R') => {
                self.current_tab = 9;
                self.selected_index = 0;
            } // Roadmap
            KeyCode::Char('I') => {
                self.current_tab = 10;
                self.selected_index = 0;
            } // Ideation
            KeyCode::Char('W') => {
                self.current_tab = 11;
                self.selected_index = 0;
            } // Worktrees
            KeyCode::Char('G') => {
                self.current_tab = 12;
                self.selected_index = 0;
            } // GitHub Issues
            KeyCode::Char('P') => {
                self.current_tab = 13;
                self.selected_index = 0;
            } // GitHub PRs
            KeyCode::Char('S') => {
                self.current_tab = 14;
                self.selected_index = 0;
            } // Stacks
            KeyCode::Char('X') => {
                self.current_tab = 15;
                self.selected_index = 0;
            } // Context
            KeyCode::Char('L') => {
                self.current_tab = 16;
                self.selected_index = 0;
            } // Changelog

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
                if self.current_tab == 2 && self.kanban_column > 0 {
                    self.kanban_column -= 1;
                }
                // Context sub-tab switching
                if self.current_tab == 15 && self.context_sub_tab > 0 {
                    self.context_sub_tab -= 1;
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if self.current_tab == 2 && self.kanban_column < 4 {
                    self.kanban_column += 1;
                }
                if self.current_tab == 15 && self.context_sub_tab < 1 {
                    self.context_sub_tab += 1;
                }
            }

            // Toggle expand/collapse in changelog
            KeyCode::Enter => {
                if self.current_tab == 16 && self.selected_index < self.changelog.len() {
                    self.changelog[self.selected_index].expanded =
                        !self.changelog[self.selected_index].expanded;
                }
            }

            // Help
            KeyCode::Char('?') => self.show_help = true,

            // Refresh (reload config text)
            KeyCode::Char('r') => {
                self.config_text = load_config_text();
            }

            _ => {}
        }
    }

    /// Returns the length of the primary list for the current tab.
    fn current_list_len(&self) -> usize {
        match self.current_tab {
            0 => self.agents.len(), // dashboard agent panel
            1 => self.agents.len(),
            2 => self.beads.len(),
            3 => self.sessions.len(),
            4 => self.convoys.len(),
            5 => self.costs.len(),
            6 => 0,
            7 => 0,
            8 => self.mcp_servers.len(),
            9 => self.roadmap_items.len(),
            10 => self.ideas.len(),
            11 => self.worktrees.len(),
            12 => self.github_issues.len(),
            13 => self.github_prs.len(),
            14 => self.stacks.len(),
            15 => self.memory_entries.len(),
            16 => self.changelog.len(),
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
// Parsers for API string→enum conversion
// ---------------------------------------------------------------------------

fn parse_role(s: &str) -> AgentRole {
    match s.to_lowercase().as_str() {
        "mayor" => AgentRole::Mayor,
        "deacon" => AgentRole::Deacon,
        "witness" => AgentRole::Witness,
        "refinery" => AgentRole::Refinery,
        "polecat" => AgentRole::Polecat,
        "coder" => AgentRole::Coder,
        "planner" => AgentRole::Planner,
        _ => AgentRole::Crew,
    }
}

fn parse_agent_status(s: &str) -> AgentStatus {
    match s.to_lowercase().as_str() {
        "active" => AgentStatus::Active,
        "idle" => AgentStatus::Idle,
        "pending" => AgentStatus::Pending,
        "stopped" => AgentStatus::Stopped,
        _ => AgentStatus::Idle,
    }
}

fn parse_bead_status(s: &str) -> BeadStatus {
    match s.to_lowercase().as_str() {
        "backlog" => BeadStatus::Backlog,
        "hooked" => BeadStatus::Hooked,
        "slung" => BeadStatus::Slung,
        "review" => BeadStatus::Review,
        "done" => BeadStatus::Done,
        _ => BeadStatus::Backlog,
    }
}

fn parse_lane(s: &str) -> Lane {
    match s.to_lowercase().as_str() {
        "critical" => Lane::Critical,
        "experimental" => Lane::Experimental,
        _ => Lane::Standard,
    }
}

fn parse_cli_type(s: &str) -> CliType {
    match s.to_lowercase().as_str() {
        "claude" => CliType::Claude,
        "codex" => CliType::Codex,
        "gemini" => CliType::Gemini,
        _ => CliType::Claude,
    }
}

fn parse_convoy_status(s: &str) -> ConvoyStatus {
    match s.to_lowercase().as_str() {
        "active" => ConvoyStatus::Active,
        "completed" => ConvoyStatus::Completed,
        "forming" => ConvoyStatus::Forming,
        _ => ConvoyStatus::Forming,
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
        BeadInfo {
            id: "bd-001".into(),
            title: "Set up CI pipeline".into(),
            status: BeadStatus::Done,
            lane: Lane::Standard,
        },
        BeadInfo {
            id: "bd-002".into(),
            title: "Implement auth module".into(),
            status: BeadStatus::Slung,
            lane: Lane::Critical,
        },
        BeadInfo {
            id: "bd-003".into(),
            title: "Add unit tests for core".into(),
            status: BeadStatus::Slung,
            lane: Lane::Standard,
        },
        BeadInfo {
            id: "bd-004".into(),
            title: "Design TUI layout".into(),
            status: BeadStatus::Review,
            lane: Lane::Standard,
        },
        BeadInfo {
            id: "bd-005".into(),
            title: "Write API docs".into(),
            status: BeadStatus::Hooked,
            lane: Lane::Experimental,
        },
        BeadInfo {
            id: "bd-006".into(),
            title: "Refactor config loader".into(),
            status: BeadStatus::Backlog,
            lane: Lane::Standard,
        },
        BeadInfo {
            id: "bd-007".into(),
            title: "Add MCP transport".into(),
            status: BeadStatus::Backlog,
            lane: Lane::Critical,
        },
        BeadInfo {
            id: "bd-008".into(),
            title: "Optimize token usage".into(),
            status: BeadStatus::Backlog,
            lane: Lane::Experimental,
        },
        BeadInfo {
            id: "bd-009".into(),
            title: "Setup monitoring".into(),
            status: BeadStatus::Done,
            lane: Lane::Standard,
        },
        BeadInfo {
            id: "bd-010".into(),
            title: "Convoy orchestration".into(),
            status: BeadStatus::Hooked,
            lane: Lane::Critical,
        },
    ]
}

fn demo_sessions() -> Vec<SessionInfo> {
    vec![
        SessionInfo {
            id: "sess-01".into(),
            agent: "mayor-alpha".into(),
            cli_type: CliType::Claude,
            status: "running".into(),
            duration: "12m 34s".into(),
            cpu: "2.1%".into(),
        },
        SessionInfo {
            id: "sess-02".into(),
            agent: "deacon-bravo".into(),
            cli_type: CliType::Claude,
            status: "running".into(),
            duration: "8m 12s".into(),
            cpu: "1.4%".into(),
        },
        SessionInfo {
            id: "sess-03".into(),
            agent: "crew-charlie".into(),
            cli_type: CliType::Codex,
            status: "idle".into(),
            duration: "45m 01s".into(),
            cpu: "0.0%".into(),
        },
        SessionInfo {
            id: "sess-04".into(),
            agent: "crew-delta".into(),
            cli_type: CliType::Gemini,
            status: "starting".into(),
            duration: "0m 03s".into(),
            cpu: "0.5%".into(),
        },
    ]
}

fn demo_convoys() -> Vec<ConvoyInfo> {
    vec![
        ConvoyInfo {
            name: "auth-feature".into(),
            status: ConvoyStatus::Active,
            bead_count: 3,
            progress: 66,
        },
        ConvoyInfo {
            name: "ci-setup".into(),
            status: ConvoyStatus::Completed,
            bead_count: 2,
            progress: 100,
        },
        ConvoyInfo {
            name: "api-docs".into(),
            status: ConvoyStatus::Forming,
            bead_count: 4,
            progress: 10,
        },
    ]
}

fn demo_costs() -> Vec<CostRow> {
    vec![
        CostRow {
            provider: "Anthropic".into(),
            model: "claude-opus-4".into(),
            input_tokens: 125_000,
            output_tokens: 42_000,
            cost_usd: 3.45,
        },
        CostRow {
            provider: "Anthropic".into(),
            model: "claude-sonnet-4".into(),
            input_tokens: 310_000,
            output_tokens: 98_000,
            cost_usd: 2.12,
        },
        CostRow {
            provider: "OpenAI".into(),
            model: "o3".into(),
            input_tokens: 80_000,
            output_tokens: 25_000,
            cost_usd: 1.80,
        },
        CostRow {
            provider: "Google".into(),
            model: "gemini-2.5-pro".into(),
            input_tokens: 50_000,
            output_tokens: 15_000,
            cost_usd: 0.65,
        },
    ]
}

fn demo_mcp() -> Vec<McpServerInfo> {
    vec![
        McpServerInfo {
            name: "filesystem".into(),
            transport: "stdio".into(),
            status: "connected".into(),
            tools: 8,
        },
        McpServerInfo {
            name: "git".into(),
            transport: "stdio".into(),
            status: "connected".into(),
            tools: 12,
        },
        McpServerInfo {
            name: "postgres".into(),
            transport: "sse".into(),
            status: "disconnected".into(),
            tools: 5,
        },
        McpServerInfo {
            name: "web-search".into(),
            transport: "stdio".into(),
            status: "connected".into(),
            tools: 3,
        },
    ]
}

fn demo_activity() -> Vec<ActivityEntry> {
    let now = Utc::now();
    vec![
        ActivityEntry {
            timestamp: now,
            message: "mayor-alpha hooked bead bd-002".into(),
        },
        ActivityEntry {
            timestamp: now,
            message: "deacon-bravo completed review on bd-004".into(),
        },
        ActivityEntry {
            timestamp: now,
            message: "crew-charlie went idle".into(),
        },
        ActivityEntry {
            timestamp: now,
            message: "convoy auth-feature progress: 66%".into(),
        },
        ActivityEntry {
            timestamp: now,
            message: "crew-delta spawned with gemini-2.5-pro".into(),
        },
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

fn demo_worktrees() -> Vec<WorktreeInfo> {
    vec![
        WorktreeInfo {
            id: "wt-01".into(),
            path: "/tmp/auto-tundra/feat-auth".into(),
            branch: "feat/auth-module".into(),
            bead_id: "bd-002".into(),
            status: "active".into(),
        },
        WorktreeInfo {
            id: "wt-02".into(),
            path: "/tmp/auto-tundra/fix-ci".into(),
            branch: "fix/ci-pipeline".into(),
            bead_id: "bd-001".into(),
            status: "active".into(),
        },
        WorktreeInfo {
            id: "wt-03".into(),
            path: "/tmp/auto-tundra/docs".into(),
            branch: "docs/api".into(),
            bead_id: "bd-005".into(),
            status: "stale".into(),
        },
    ]
}

fn demo_github_issues() -> Vec<GithubIssueInfo> {
    vec![
        GithubIssueInfo {
            number: 42,
            title: "Add WebSocket support".into(),
            labels: vec!["enhancement".into()],
            assignee: Some("mayor-alpha".into()),
            state: "open".into(),
            created: "2026-02-18".into(),
        },
        GithubIssueInfo {
            number: 41,
            title: "Fix config parsing edge case".into(),
            labels: vec!["bug".into()],
            assignee: None,
            state: "open".into(),
            created: "2026-02-17".into(),
        },
        GithubIssueInfo {
            number: 40,
            title: "Update dependencies".into(),
            labels: vec!["chore".into()],
            assignee: Some("crew-charlie".into()),
            state: "closed".into(),
            created: "2026-02-15".into(),
        },
    ]
}

fn demo_github_prs() -> Vec<GithubPrInfo> {
    vec![
        GithubPrInfo {
            number: 15,
            title: "feat: add agent orchestration".into(),
            author: "mayor-alpha".into(),
            status: "open".into(),
            reviewers: vec!["deacon-bravo".into()],
            created: "2026-02-20".into(),
        },
        GithubPrInfo {
            number: 14,
            title: "fix: convoy status tracking".into(),
            author: "crew-charlie".into(),
            status: "merged".into(),
            reviewers: vec!["witness-echo".into()],
            created: "2026-02-19".into(),
        },
    ]
}

fn demo_roadmap() -> Vec<RoadmapItemInfo> {
    vec![
        RoadmapItemInfo {
            id: "rm-01".into(),
            title: "Multi-provider support".into(),
            description: "Support Claude, Codex, Gemini".into(),
            status: "in_progress".into(),
            priority: "high".into(),
        },
        RoadmapItemInfo {
            id: "rm-02".into(),
            title: "WebSocket streaming".into(),
            description: "Real-time terminal output".into(),
            status: "planned".into(),
            priority: "high".into(),
        },
        RoadmapItemInfo {
            id: "rm-03".into(),
            title: "Plugin system".into(),
            description: "MCP-based plugin architecture".into(),
            status: "planned".into(),
            priority: "medium".into(),
        },
        RoadmapItemInfo {
            id: "rm-04".into(),
            title: "Team collaboration".into(),
            description: "Multi-user workspace".into(),
            status: "backlog".into(),
            priority: "low".into(),
        },
    ]
}

fn demo_ideas() -> Vec<IdeaInfo> {
    vec![
        IdeaInfo {
            id: "idea-01".into(),
            title: "Auto-retry failed beads".into(),
            description: "Retry with exponential backoff".into(),
            category: "performance".into(),
            impact: "high".into(),
            effort: "medium".into(),
        },
        IdeaInfo {
            id: "idea-02".into(),
            title: "Cost alert thresholds".into(),
            description: "Notify when spend exceeds limit".into(),
            category: "cost".into(),
            impact: "medium".into(),
            effort: "low".into(),
        },
        IdeaInfo {
            id: "idea-03".into(),
            title: "Git worktree auto-cleanup".into(),
            description: "Remove stale worktrees on bead completion".into(),
            category: "quality".into(),
            impact: "low".into(),
            effort: "low".into(),
        },
    ]
}

fn demo_stacks() -> Vec<StackNodeInfo> {
    vec![
        StackNodeInfo {
            id: "bd-006".into(),
            title: "Build agent executor".into(),
            phase: "In Progress".into(),
            git_branch: Some("feat/agent-executor".into()),
            pr_number: Some(41),
            depth: 0,
        },
        StackNodeInfo {
            id: "bd-007".into(),
            title: "MCP tool integration".into(),
            phase: "In Progress".into(),
            git_branch: Some("feat/mcp-integration".into()),
            pr_number: Some(39),
            depth: 1,
        },
        StackNodeInfo {
            id: "bd-010".into(),
            title: "Review agent executor v1".into(),
            phase: "AI Review".into(),
            git_branch: Some("feat/executor-review".into()),
            pr_number: None,
            depth: 2,
        },
    ]
}

fn demo_changelog() -> Vec<ChangelogEntryInfo> {
    vec![
        ChangelogEntryInfo {
            version: "0.3.0".into(),
            date: "2026-02-21".into(),
            sections: vec![
                (
                    "Added".into(),
                    vec!["TUI dashboard".into(), "Agent orchestration".into()],
                ),
                ("Fixed".into(), vec!["Config parser edge case".into()]),
            ],
            expanded: true,
        },
        ChangelogEntryInfo {
            version: "0.2.0".into(),
            date: "2026-02-15".into(),
            sections: vec![(
                "Added".into(),
                vec!["MCP transport".into(), "Convoy system".into()],
            )],
            expanded: false,
        },
    ]
}

fn demo_memory() -> Vec<MemoryEntryInfo> {
    vec![
        MemoryEntryInfo {
            id: "mem-01".into(),
            category: "pattern".into(),
            content: "Use flume channels for async communication".into(),
            created_at: "2026-02-20".into(),
        },
        MemoryEntryInfo {
            id: "mem-02".into(),
            category: "convention".into(),
            content: "API keys via env vars only".into(),
            created_at: "2026-02-18".into(),
        },
    ]
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
