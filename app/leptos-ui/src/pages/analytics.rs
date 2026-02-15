use leptos::prelude::*;

use crate::state::use_app_state;
use crate::types::AgentStatus;

#[component]
pub fn AnalyticsPage() -> impl IntoView {
    let state = use_app_state();
    let agents = state.agents;
    let beads = state.beads;

    let beads_done = move || {
        beads.get().iter()
            .filter(|b| b.status == crate::types::BeadStatus::Done)
            .count()
    };

    let beads_in_progress = move || {
        beads.get().iter()
            .filter(|b| b.status == crate::types::BeadStatus::InProgress)
            .count()
    };

    let beads_pending = move || {
        beads.get().iter()
            .filter(|b| b.status == crate::types::BeadStatus::Pending)
            .count()
    };

    view! {
        <div class="page-header">
            <h2>"Analytics"</h2>
        </div>

        <div class="kpi-grid">
            <div class="kpi-card">
                <div class="value">{move || agents.get().len()}</div>
                <div class="label">"Total Agents"</div>
            </div>
            <div class="kpi-card">
                <div class="value">{move || agents.get().iter().filter(|a| a.status == AgentStatus::Active).count()}</div>
                <div class="label">"Active Now"</div>
            </div>
            <div class="kpi-card">
                <div class="value">{beads_done}</div>
                <div class="label">"Beads Done"</div>
            </div>
            <div class="kpi-card">
                <div class="value">{beads_in_progress}</div>
                <div class="label">"In Progress"</div>
            </div>
        </div>

        <div class="section">
            <h3>"Bead Status Breakdown"</h3>
            <div class="activity-feed">
                <div class="activity-item">
                    {move || format!("Pending: {}", beads_pending())}
                </div>
                <div class="activity-item">
                    {move || format!("In Progress: {}", beads_in_progress())}
                </div>
                <div class="activity-item">
                    {move || format!("Done: {}", beads_done())}
                </div>
            </div>
        </div>

        <div class="section">
            <h3>"Agent Token Usage"</h3>
            <div class="activity-feed">
                {move || agents.get().into_iter().map(|a| {
                    view! {
                        <div class="activity-item">
                            {format!("{}: {} tokens (${:.2})", a.name, a.tokens_used, a.cost_usd)}
                        </div>
                    }
                }).collect::<Vec<_>>()}
            </div>
        </div>
    }
}
