use leptos::prelude::*;

use crate::types::AgentStatus;

#[component]
pub fn AgentCard(
    name: String,
    role: String,
    model: String,
    status: AgentStatus,
    tokens: u64,
    cost: f64,
) -> impl IntoView {
    let glyph = status.glyph();
    let glyph_class = status.css_class();

    view! {
        <div class="agent-card">
            <div class="agent-header">
                <span class="agent-name">{name}</span>
                <span class=format!("agent-status {}", glyph_class)>{glyph}</span>
            </div>
            <div class="agent-role">{role}</div>
            <div class="agent-model">{model}</div>
            <div style="margin-top: 8px; font-size: 0.8em; color: #8b949e;">
                {format!("{} tokens | ${:.2}", tokens, cost)}
            </div>
        </div>
    }
}
