use leptos::prelude::*;

use crate::components::bead_card::BeadCard;
use crate::state::use_app_state;
use crate::types::Lane;

#[component]
pub fn BeadsPage() -> impl IntoView {
    let state = use_app_state();
    let beads = state.beads;

    let lanes = vec![
        (Lane::Backlog, "Backlog"),
        (Lane::Hooked, "Hooked"),
        (Lane::Slung, "Slung"),
        (Lane::Review, "Review"),
        (Lane::Done, "Done"),
    ];

    view! {
        <div class="page-header">
            <h2>"Beads"</h2>
        </div>
        <div class="kanban">
            {lanes.into_iter().map(|(lane, label)| {
                let lane_clone = lane.clone();
                let lane_for_count = lane_clone.clone();
                let beads_in_lane = move || {
                    beads.get().into_iter()
                        .filter(|b| b.lane == lane_clone)
                        .collect::<Vec<_>>()
                };
                let beads_in_lane_render = move || {
                    beads.get().into_iter()
                        .filter(|b| b.lane == lane_for_count)
                        .collect::<Vec<_>>()
                };
                let count = move || beads_in_lane().len();
                view! {
                    <div class="kanban-column">
                        <h3>
                            {label}
                            " "
                            <span class="count">"(" {count} ")"</span>
                        </h3>
                        {move || beads_in_lane_render().into_iter().map(|bead| {
                            view! {
                                <BeadCard
                                    id=bead.id.clone()
                                    title=bead.title.clone()
                                    status=bead.status.to_string()
                                    description=bead.description.clone()
                                />
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}
