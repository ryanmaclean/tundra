use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DisplayMode {
    Standard,
    Foil,
    Vt100,
}

impl DisplayMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            DisplayMode::Standard => "standard",
            DisplayMode::Foil => "foil",
            DisplayMode::Vt100 => "vt100",
        }
    }
}

use crate::types::{
    demo_agents, demo_beads, demo_claude_sessions, demo_context_entries, demo_convoys, demo_costs,
    demo_github_issues, demo_github_prs, demo_ideas, demo_kpis, demo_mcp_servers, demo_roadmap,
    demo_sessions, demo_worktrees, AgentResponse, BeadResponse, ClaudeSession, ContextEntry,
    ConvoyResponse, CostEntry, GithubIssue, GithubPr, IdeaResponse, KpiResponse, McpServerEntry,
    RoadmapPhase, SessionResponse, StatusResponse, WorktreeEntry,
};

#[derive(Clone)]
pub struct AppState {
    pub agents: ReadSignal<Vec<AgentResponse>>,
    pub set_agents: WriteSignal<Vec<AgentResponse>>,

    pub beads: ReadSignal<Vec<BeadResponse>>,
    pub set_beads: WriteSignal<Vec<BeadResponse>>,
    pub kpis: ReadSignal<Vec<KpiResponse>>,
    pub sessions: ReadSignal<Vec<SessionResponse>>,
    pub convoys: ReadSignal<Vec<ConvoyResponse>>,
    pub costs: ReadSignal<Vec<CostEntry>>,
    pub mcp_servers: ReadSignal<Vec<McpServerEntry>>,
    pub status: ReadSignal<StatusResponse>,
    pub set_status: WriteSignal<StatusResponse>,

    pub ideas: ReadSignal<Vec<IdeaResponse>>,
    pub set_ideas: WriteSignal<Vec<IdeaResponse>>,
    pub roadmap: ReadSignal<Vec<RoadmapPhase>>,
    pub context_entries: ReadSignal<Vec<ContextEntry>>,
    pub worktrees: ReadSignal<Vec<WorktreeEntry>>,
    pub github_issues: ReadSignal<Vec<GithubIssue>>,
    pub set_github_issues: WriteSignal<Vec<GithubIssue>>,
    pub github_prs: ReadSignal<Vec<GithubPr>>,
    pub set_github_prs: WriteSignal<Vec<GithubPr>>,
    pub is_demo: ReadSignal<bool>,
    pub set_is_demo: WriteSignal<bool>,

    pub claude_sessions: ReadSignal<Vec<ClaudeSession>>,
    /// Currently dragged bead ID for kanban drag-and-drop.
    pub dragging_bead: ReadSignal<Option<String>>,
    pub set_dragging_bead: WriteSignal<Option<String>>,
    pub display_mode: ReadSignal<DisplayMode>,
    pub set_display_mode: WriteSignal<DisplayMode>,
    pub reduce_motion: ReadSignal<bool>,
    pub set_reduce_motion: WriteSignal<bool>,

    /// Active project name (fetched from API on startup).
    pub project_name: ReadSignal<String>,
    pub set_project_name: WriteSignal<String>,
}

pub fn provide_app_state() {
    // Removed set_agents and set_status from exports

    let (agents, set_agents) = signal(demo_agents());

    let (beads, set_beads) = signal(demo_beads());
    let (kpis, _) = signal(demo_kpis());
    let (sessions, _) = signal(demo_sessions());
    let (convoys, _) = signal(demo_convoys());
    let (costs, _) = signal(demo_costs());
    let (mcp_servers, _) = signal(demo_mcp_servers());
    let (status, set_status) = signal(StatusResponse {
        daemon_running: true,
        active_agents: 3,
        total_beads: 20,
        uptime_secs: 3_621,
    });
    let (ideas, set_ideas) = signal(demo_ideas());
    let (roadmap, _) = signal(demo_roadmap());
    let (context_entries, _) = signal(demo_context_entries());
    let (worktrees, _) = signal(demo_worktrees());
    let (github_issues, set_github_issues) = signal(demo_github_issues());
    let (github_prs, set_github_prs) = signal(demo_github_prs());
    let (is_demo, set_is_demo) = signal(true);

    let (claude_sessions, _) = signal(demo_claude_sessions());
    let (dragging_bead, set_dragging_bead) = signal(None::<String>);
    let (display_mode, set_display_mode) = signal(DisplayMode::Standard);
    let (reduce_motion, set_reduce_motion) = signal(false);
    let (project_name, set_project_name) = signal(String::from("auto-tundra"));

    let state = AppState {
        set_agents,
        set_status,

        agents,

        beads,
        set_beads,
        kpis,
        sessions,
        convoys,
        costs,
        mcp_servers,
        status,

        ideas,
        set_ideas,
        roadmap,
        context_entries,
        worktrees,
        github_issues,
        set_github_issues,
        github_prs,
        set_github_prs,
        is_demo,
        set_is_demo,

        claude_sessions,
        dragging_bead,
        set_dragging_bead,
        display_mode,
        set_display_mode,
        reduce_motion,
        set_reduce_motion,
        project_name,
        set_project_name,
    };

    provide_context(state);

    // Startup hydration: attempt to load live backend data.
    // If successful, switch off demo mode without waiting for manual refresh.
    leptos::task::spawn_local(async move {
        let mut saw_live_data = false;

        if let Ok(api_beads) = crate::api::fetch_beads().await {
            let real: Vec<crate::types::BeadResponse> = api_beads
                .iter()
                .map(crate::pages::beads::api_bead_to_bead_response)
                .collect();
            if !real.is_empty() {
                set_beads.set(real);
                saw_live_data = true;
            }
        }

        if let Ok(api_agents) = crate::api::fetch_agents().await {
            let real_agents: Vec<crate::types::AgentResponse> = api_agents
                .iter()
                .map(|a| {
                    let status = match a.status.as_str() {
                        "active" => crate::types::AgentStatus::Active,
                        "idle" => crate::types::AgentStatus::Idle,
                        "pending" => crate::types::AgentStatus::Pending,
                        "stopped" => crate::types::AgentStatus::Stopped,
                        _ => crate::types::AgentStatus::Unknown,
                    };
                    crate::types::AgentResponse {
                        id: a.id.clone(),
                        name: a.name.clone(),
                        role: a.role.clone(),
                        model: String::new(),
                        status,
                        tokens_used: 0,
                        cost_usd: 0.0,
                    }
                })
                .collect();
            if !real_agents.is_empty() {
                set_agents.set(real_agents);
                saw_live_data = true;
            }
        }

        if let Ok(st) = crate::api::fetch_status().await {
            set_status.set(crate::types::StatusResponse {
                daemon_running: true,
                active_agents: st.agent_count as u32,
                total_beads: st.bead_count as u32,
                uptime_secs: st.uptime_secs,
            });
            saw_live_data = true;
        }

        if saw_live_data {
            set_is_demo.set(false);
        }

        // Fetch active project name for breadcrumb / sidebar
        if let Ok(projects) = crate::api::fetch_projects().await {
            if let Some(active) = projects.iter().find(|p| p.is_active) {
                set_project_name.set(active.name.clone());
            }
        }
    });
}

pub fn use_app_state() -> AppState {
    use_context::<AppState>().expect(
        "AppState not provided — ensure provide_app_state() is called in a parent component",
    )
}
