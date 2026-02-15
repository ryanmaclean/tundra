use leptos::prelude::*;

use crate::types::{
    AgentResponse, BeadResponse, ConvoyResponse, CostEntry, KpiResponse, McpServerEntry,
    SessionResponse, StatusResponse,
    demo_agents, demo_beads, demo_convoys, demo_costs, demo_kpis, demo_mcp_servers, demo_sessions,
};

#[derive(Clone)]
pub struct AppState {
    pub agents: ReadSignal<Vec<AgentResponse>>,
    pub beads: ReadSignal<Vec<BeadResponse>>,
    pub kpis: ReadSignal<Vec<KpiResponse>>,
    pub sessions: ReadSignal<Vec<SessionResponse>>,
    pub convoys: ReadSignal<Vec<ConvoyResponse>>,
    pub costs: ReadSignal<Vec<CostEntry>>,
    pub mcp_servers: ReadSignal<Vec<McpServerEntry>>,
    pub status: ReadSignal<StatusResponse>,
}

pub fn provide_app_state() {
    let (agents, _) = signal(demo_agents());
    let (beads, _) = signal(demo_beads());
    let (kpis, _) = signal(demo_kpis());
    let (sessions, _) = signal(demo_sessions());
    let (convoys, _) = signal(demo_convoys());
    let (costs, _) = signal(demo_costs());
    let (mcp_servers, _) = signal(demo_mcp_servers());
    let (status, _) = signal(StatusResponse {
        daemon_running: true,
        active_agents: 3,
        total_beads: 8,
        uptime_secs: 3_621,
    });

    let state = AppState {
        agents,
        beads,
        kpis,
        sessions,
        convoys,
        costs,
        mcp_servers,
        status,
    };

    provide_context(state);
}

pub fn use_app_state() -> AppState {
    expect_context::<AppState>()
}
