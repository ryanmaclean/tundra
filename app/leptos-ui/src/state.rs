use leptos::prelude::*;

use crate::types::{
    AgentResponse, BeadResponse, ClaudeSession, ContextEntry, ConvoyResponse, CostEntry,
    GithubIssue, GithubPr, IdeaResponse, KpiResponse, McpServerEntry, RoadmapPhase,
    SessionResponse, StatusResponse, WorktreeEntry,
    demo_agents, demo_beads, demo_claude_sessions, demo_context_entries, demo_convoys,
    demo_costs, demo_github_issues, demo_github_prs, demo_ideas, demo_kpis, demo_mcp_servers,
    demo_roadmap, demo_sessions, demo_worktrees,
};

#[derive(Clone)]
pub struct AppState {
    pub agents: ReadSignal<Vec<AgentResponse>>,
    pub beads: ReadSignal<Vec<BeadResponse>>,
    pub set_beads: WriteSignal<Vec<BeadResponse>>,
    pub kpis: ReadSignal<Vec<KpiResponse>>,
    pub sessions: ReadSignal<Vec<SessionResponse>>,
    pub convoys: ReadSignal<Vec<ConvoyResponse>>,
    pub costs: ReadSignal<Vec<CostEntry>>,
    pub mcp_servers: ReadSignal<Vec<McpServerEntry>>,
    pub status: ReadSignal<StatusResponse>,
    pub ideas: ReadSignal<Vec<IdeaResponse>>,
    pub set_ideas: WriteSignal<Vec<IdeaResponse>>,
    pub roadmap: ReadSignal<Vec<RoadmapPhase>>,
    pub context_entries: ReadSignal<Vec<ContextEntry>>,
    pub worktrees: ReadSignal<Vec<WorktreeEntry>>,
    pub github_issues: ReadSignal<Vec<GithubIssue>>,
    pub github_prs: ReadSignal<Vec<GithubPr>>,
    pub claude_sessions: ReadSignal<Vec<ClaudeSession>>,
    /// Currently dragged bead ID for kanban drag-and-drop.
    pub dragging_bead: ReadSignal<Option<String>>,
    pub set_dragging_bead: WriteSignal<Option<String>>,
}

pub fn provide_app_state() {
    let (agents, _) = signal(demo_agents());
    let (beads, set_beads) = signal(demo_beads());
    let (kpis, _) = signal(demo_kpis());
    let (sessions, _) = signal(demo_sessions());
    let (convoys, _) = signal(demo_convoys());
    let (costs, _) = signal(demo_costs());
    let (mcp_servers, _) = signal(demo_mcp_servers());
    let (status, _) = signal(StatusResponse {
        daemon_running: true,
        active_agents: 3,
        total_beads: 20,
        uptime_secs: 3_621,
    });
    let (ideas, set_ideas) = signal(demo_ideas());
    let (roadmap, _) = signal(demo_roadmap());
    let (context_entries, _) = signal(demo_context_entries());
    let (worktrees, _) = signal(demo_worktrees());
    let (github_issues, _) = signal(demo_github_issues());
    let (github_prs, _) = signal(demo_github_prs());
    let (claude_sessions, _) = signal(demo_claude_sessions());
    let (dragging_bead, set_dragging_bead) = signal(None::<String>);

    let state = AppState {
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
        github_prs,
        claude_sessions,
        dragging_bead,
        set_dragging_bead,
    };

    provide_context(state);
}

pub fn use_app_state() -> AppState {
    expect_context::<AppState>()
}
