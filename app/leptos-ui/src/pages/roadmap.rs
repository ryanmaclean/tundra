use leptos::prelude::*;

use crate::state::use_app_state;

#[component]
pub fn RoadmapPage() -> impl IntoView {
    let state = use_app_state();
    let phases = state.roadmap;

    view! {
        <div class="page-header">
            <h2>"Roadmap"</h2>
        </div>

        <div class="roadmap-timeline">
            {move || phases.get().into_iter().map(|phase| {
                let status_class = match phase.status.as_str() {
                    "completed" => "phase-completed",
                    "active" => "phase-active",
                    "planned" => "phase-planned",
                    _ => "phase-planned",
                };
                let status_label = match phase.status.as_str() {
                    "completed" => "Completed",
                    "active" => "Active",
                    "planned" => "Planned",
                    _ => "Unknown",
                };
                let features_view = phase.features.iter().map(|feat| {
                    view! {
                        <li class="phase-feature">{feat.clone()}</li>
                    }
                }).collect::<Vec<_>>();
                view! {
                    <div class={format!("phase-card {}", status_class)}>
                        <div class="phase-header">
                            <h3 class="phase-name">{phase.name.clone()}</h3>
                            <span class="phase-status-badge">{status_label}</span>
                        </div>
                        <div class="phase-date">{phase.date_range.clone()}</div>
                        <ul class="phase-features">
                            {features_view}
                        </ul>
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}
