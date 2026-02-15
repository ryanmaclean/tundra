use leptos::prelude::*;

use crate::state::use_app_state;

#[component]
pub fn CostsPage() -> impl IntoView {
    let state = use_app_state();
    let costs = state.costs;

    let total_cost = move || {
        costs.get().iter().map(|c| c.cost_usd).sum::<f64>()
    };

    let total_tokens = move || {
        costs.get().iter().map(|c| c.tokens).sum::<u64>()
    };

    view! {
        <div class="page-header">
            <h2>"Cost Breakdown"</h2>
        </div>
        <div class="kpi-grid" style="grid-template-columns: repeat(2, 1fr);">
            <div class="kpi-card">
                <div class="value">{move || format!("${:.2}", total_cost())}</div>
                <div class="label">"Total Cost"</div>
            </div>
            <div class="kpi-card">
                <div class="value">{move || format!("{}", total_tokens())}</div>
                <div class="label">"Total Tokens"</div>
            </div>
        </div>
        <table class="data-table">
            <thead>
                <tr>
                    <th>"Agent"</th>
                    <th>"Model"</th>
                    <th>"Tokens"</th>
                    <th>"Cost (USD)"</th>
                </tr>
            </thead>
            <tbody>
                {move || costs.get().into_iter().map(|c| {
                    view! {
                        <tr>
                            <td>{c.agent_name}</td>
                            <td>{c.model}</td>
                            <td>{format!("{}", c.tokens)}</td>
                            <td>{format!("${:.2}", c.cost_usd)}</td>
                        </tr>
                    }
                }).collect::<Vec<_>>()}
            </tbody>
        </table>
    }
}
