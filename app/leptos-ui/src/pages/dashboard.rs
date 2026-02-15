use leptos::prelude::*;

use crate::components::kpi_card::KpiCard;
use crate::state::use_app_state;
use crate::types::AgentStatus;

#[component]
pub fn DashboardPage() -> impl IntoView {
    let state = use_app_state();

    let agents = state.agents;
    let kpis = state.kpis;

    let active_count = move || {
        agents.get().iter().filter(|a| a.status == AgentStatus::Active).count()
    };

    let idle_count = move || {
        agents.get().iter().filter(|a| a.status == AgentStatus::Idle).count()
    };

    view! {
        <div class="page-header">
            <h2>"Dashboard"</h2>
        </div>

        <div class="kpi-grid">
            {move || kpis.get().into_iter().map(|kpi| {
                view! { <KpiCard label=kpi.label value=kpi.value /> }
            }).collect::<Vec<_>>()}
        </div>

        <div class="section">
            <h3>"Agent Summary"</h3>
            <div class="activity-feed">
                <div class="activity-item">
                    <span class="glyph-active">"@ "</span>
                    {move || format!("{} active", active_count())}
                </div>
                <div class="activity-item">
                    <span class="glyph-idle">"* "</span>
                    {move || format!("{} idle", idle_count())}
                </div>
                <div class="activity-item">
                    <span class="glyph-stopped">"x "</span>
                    {move || {
                        let stopped = agents.get().iter()
                            .filter(|a| a.status == AgentStatus::Stopped)
                            .count();
                        format!("{} stopped", stopped)
                    }}
                </div>
            </div>
        </div>

        <div class="section">
            <h3>"Recent Activity"</h3>
            <div class="activity-feed">
                <div class="activity-item">
                    <span class="timestamp">"09:31:12"</span>
                    "Architect started session scaffold-sprint"
                </div>
                <div class="activity-item">
                    <span class="timestamp">"09:32:45"</span>
                    "Coder-A picked up bead-003: Build agent executor"
                </div>
                <div class="activity-item">
                    <span class="timestamp">"09:35:20"</span>
                    "Coder-B picked up bead-004: MCP tool integration"
                </div>
                <div class="activity-item">
                    <span class="timestamp">"09:40:01"</span>
                    "Reviewer queued bead-005 for review"
                </div>
                <div class="activity-item">
                    <span class="timestamp">"09:42:33"</span>
                    "bead-001: Setup project scaffolding -> Done"
                </div>
            </div>
        </div>
    }
}
