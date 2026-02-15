use leptos::prelude::*;

use crate::components::agent_card::AgentCard;
use crate::state::use_app_state;

#[component]
pub fn AgentsPage() -> impl IntoView {
    let state = use_app_state();
    let agents = state.agents;

    view! {
        <div class="page-header">
            <h2>"Agent Terminals"</h2>
        </div>
        <div class="agent-grid">
            {move || agents.get().into_iter().map(|agent| {
                view! {
                    <AgentCard
                        name=agent.name.clone()
                        role=agent.role.clone()
                        model=agent.model.clone()
                        status=agent.status.clone()
                        tokens=agent.tokens_used
                        cost=agent.cost_usd
                    />
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}
