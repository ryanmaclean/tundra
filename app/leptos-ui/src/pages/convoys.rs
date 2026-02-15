use leptos::prelude::*;

use crate::state::use_app_state;

#[component]
pub fn ConvoysPage() -> impl IntoView {
    let state = use_app_state();
    let convoys = state.convoys;

    view! {
        <div class="page-header">
            <h2>"Convoys"</h2>
        </div>
        <div class="section">
            {move || convoys.get().into_iter().map(|c| {
                let pct = if c.total_beads > 0 {
                    (c.completed_beads as f64 / c.total_beads as f64 * 100.0) as u32
                } else {
                    0
                };
                let width = format!("{}%", pct);
                view! {
                    <div class="agent-card" style="margin-bottom: 12px;">
                        <div class="agent-header">
                            <span class="agent-name">{c.name}</span>
                            <span class="agent-role">{format!("{}/{} beads", c.completed_beads, c.total_beads)}</span>
                        </div>
                        <div class="progress-bar">
                            <div class="fill" style:width=width></div>
                        </div>
                        <div style="margin-top: 4px; font-size: 0.8em; color: #8b949e;">
                            {format!("{}% complete - {}", pct, c.status)}
                        </div>
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}
