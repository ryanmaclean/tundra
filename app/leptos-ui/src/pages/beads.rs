use leptos::prelude::*;
use web_sys::DragEvent;

use crate::state::use_app_state;
use crate::types::{BeadStatus, Lane};

/// Map an action to the next lane the bead should move to.
fn next_lane_for_action(action: &str, current: &Lane) -> Option<Lane> {
    match action {
        "start" => Some(Lane::InProgress),
        "recover" => Some(Lane::Planning),
        "resume" => Some(Lane::InProgress),
        "review" => Some(Lane::AiReview),
        "approve" => Some(Lane::Done),
        "reject" => Some(Lane::InProgress),
        // Generic forward movement
        "forward" => match current {
            Lane::Backlog => Some(Lane::Queue),
            Lane::Queue => Some(Lane::Planning),
            Lane::Planning => Some(Lane::InProgress),
            Lane::InProgress => Some(Lane::AiReview),
            Lane::AiReview => Some(Lane::HumanReview),
            Lane::HumanReview => Some(Lane::Done),
            Lane::Done => Some(Lane::PrCreated),
            Lane::PrCreated => None,
        },
        _ => None,
    }
}

fn status_for_lane(lane: &Lane) -> BeadStatus {
    match lane {
        Lane::Backlog => BeadStatus::Planning,
        Lane::Queue => BeadStatus::Planning,
        Lane::Planning => BeadStatus::Planning,
        Lane::InProgress => BeadStatus::InProgress,
        Lane::AiReview => BeadStatus::AiReview,
        Lane::HumanReview => BeadStatus::HumanReview,
        Lane::Done => BeadStatus::Done,
        Lane::PrCreated => BeadStatus::Done,
    }
}

fn action_for_lane(lane: &Lane) -> Option<String> {
    match lane {
        Lane::Backlog => Some("start".to_string()),
        Lane::Queue => Some("start".to_string()),
        Lane::Planning => Some("start".to_string()),
        Lane::InProgress => None,
        Lane::AiReview => None,
        Lane::HumanReview => None,
        Lane::Done => None,
        Lane::PrCreated => None,
    }
}

fn progress_for_lane(lane: &Lane) -> String {
    match lane {
        Lane::Backlog => "plan".to_string(),
        Lane::Queue => "plan".to_string(),
        Lane::Planning => "plan".to_string(),
        Lane::InProgress => "code".to_string(),
        Lane::AiReview => "qa".to_string(),
        Lane::HumanReview => "qa".to_string(),
        Lane::Done => "done".to_string(),
        Lane::PrCreated => "done".to_string(),
    }
}

#[component]
pub fn BeadsPage() -> impl IntoView {
    let state = use_app_state();
    let beads = state.beads;
    let set_beads = state.set_beads;
    let set_dragging = state.set_dragging_bead;

    // Filter state
    let (filter_category, set_filter_category) = signal("All".to_string());
    let (filter_priority, set_filter_priority) = signal("All".to_string());
    let (filter_search, set_filter_search) = signal(String::new());

    let clear_filters = move |_| {
        set_filter_category.set("All".to_string());
        set_filter_priority.set("All".to_string());
        set_filter_search.set(String::new());
    };

    let has_filters = move || {
        filter_category.get() != "All"
            || filter_priority.get() != "All"
            || !filter_search.get().is_empty()
    };

    let lanes = vec![
        (Lane::Backlog, "Backlog"),
        (Lane::Queue, "Queue"),
        (Lane::Planning, "Planning"),
        (Lane::InProgress, "In Progress"),
        (Lane::AiReview, "AI Review"),
        (Lane::HumanReview, "Human Review"),
        (Lane::Done, "Done"),
        (Lane::PrCreated, "PR Created"),
    ];

    // Move a bead to a target lane
    let move_bead = move |bead_id: String, target_lane: Lane| {
        set_beads.update(|beads_vec| {
            if let Some(bead) = beads_vec.iter_mut().find(|b| b.id == bead_id) {
                bead.lane = target_lane.clone();
                bead.status = status_for_lane(&target_lane);
                bead.progress_stage = progress_for_lane(&target_lane);
                bead.action = action_for_lane(&target_lane);
                bead.timestamp = "just now".to_string();
            }
        });
    };

    view! {
        <div class="page-header">
            <h2>"Kanban Board"</h2>
        </div>

        // Filter bar
        <div class="filter-bar">
            <div class="filter-group">
                <label class="filter-label">"Category"</label>
                <select
                    class="filter-select"
                    prop:value=move || filter_category.get()
                    on:change=move |ev| set_filter_category.set(event_target_value(&ev))
                >
                    <option value="All">"All"</option>
                    <option value="Feature">"Feature"</option>
                    <option value="Bug Fix">"Bug Fix"</option>
                    <option value="Refactoring">"Refactoring"</option>
                    <option value="Documentation">"Documentation"</option>
                    <option value="Security">"Security"</option>
                    <option value="Performance">"Performance"</option>
                    <option value="UI/UX">"UI/UX"</option>
                    <option value="Infrastructure">"Infrastructure"</option>
                    <option value="Testing">"Testing"</option>
                </select>
            </div>
            <div class="filter-group">
                <label class="filter-label">"Priority"</label>
                <select
                    class="filter-select"
                    prop:value=move || filter_priority.get()
                    on:change=move |ev| set_filter_priority.set(event_target_value(&ev))
                >
                    <option value="All">"All"</option>
                    <option value="Low">"Low"</option>
                    <option value="Medium">"Medium"</option>
                    <option value="High">"High"</option>
                    <option value="Urgent">"Urgent"</option>
                </select>
            </div>
            <div class="filter-group filter-search-group">
                <label class="filter-label">"Search"</label>
                <input
                    type="text"
                    class="filter-search"
                    placeholder="Filter by title..."
                    prop:value=move || filter_search.get()
                    on:input=move |ev| set_filter_search.set(event_target_value(&ev))
                />
            </div>
            {move || has_filters().then(|| view! {
                <button class="filter-clear-btn" on:click=clear_filters>
                    "Clear Filters"
                </button>
            })}
        </div>

        <div class="kanban">
            {lanes.into_iter().map(|( lane, label)| {
                let lane_for_count = lane.clone();
                let lane_for_render = lane.clone();
                let lane_for_drop = lane.clone();
                let lane_for_over = lane.clone();
                let move_bead_drop = move_bead.clone();

                let count = move || {
                    beads.get().into_iter()
                        .filter(|b| b.lane == lane_for_count)
                        .count()
                };

                // Drag-and-drop: on_dragover -- allow drop
                let on_dragover = move |ev: DragEvent| {
                    ev.prevent_default();
                    let _ = &lane_for_over; // keep in scope
                };

                // Drag-and-drop: on_drop -- move bead to this lane
                let on_drop = {
                    let set_dragging = set_dragging.clone();
                    move |ev: DragEvent| {
                        ev.prevent_default();
                        if let Some(dt) = ev.data_transfer() {
                            if let Ok(bead_id) = dt.get_data("text/plain") {
                                if !bead_id.is_empty() {
                                    move_bead_drop(bead_id, lane_for_drop.clone());
                                }
                            }
                        }
                        set_dragging.set(None);
                    }
                };

                view! {
                    <div
                        class="kanban-column"
                        on:dragover=on_dragover
                        on:drop=on_drop
                    >
                        <h3>
                            {label}
                            " "
                            <span class="count">"(" {count} ")"</span>
                        </h3>
                        {move || {
                            let move_bead_action = move_bead.clone();
                            let cat_filter = filter_category.get();
                            let pri_filter = filter_priority.get();
                            let search_filter = filter_search.get().to_lowercase();

                            beads.get().into_iter()
                                .filter(|b| b.lane == lane_for_render)
                                .filter(|b| {
                                    if cat_filter == "All" { return true; }
                                    b.tags.iter().any(|t| *t == cat_filter)
                                })
                                .filter(|b| {
                                    if pri_filter == "All" { return true; }
                                    b.tags.iter().any(|t| *t == pri_filter)
                                })
                                .filter(|b| {
                                    if search_filter.is_empty() { return true; }
                                    b.title.to_lowercase().contains(&search_filter)
                                })
                                .map(|bead| {
                                    let bead_id = bead.id.clone();
                                    let bead_id_drag = bead.id.clone();
                                    let set_dragging_start = set_dragging.clone();
                                    let set_dragging_end = set_dragging.clone();

                                    let status_class = match bead.status {
                                        BeadStatus::Planning => "status-planning",
                                        BeadStatus::InProgress => "status-in-progress",
                                        BeadStatus::AiReview => "status-ai-review",
                                        BeadStatus::HumanReview => "status-human-review",
                                        BeadStatus::Done => "status-done",
                                        BeadStatus::Failed => "status-failed",
                                    };

                                    let progress_stages = ["plan", "code", "qa", "done"];
                                    let current_stage = bead.progress_stage.clone();
                                    let pipeline_view = progress_stages.iter().map(|stage| {
                                        let is_active = *stage == current_stage.as_str();
                                        let is_past = progress_stages.iter().position(|s| *s == current_stage.as_str())
                                            .map(|current_pos| progress_stages.iter().position(|s| s == stage).map(|pos| pos <= current_pos).unwrap_or(false))
                                            .unwrap_or(false);
                                        let cls = if is_active {
                                            "pipeline-stage active"
                                        } else if is_past {
                                            "pipeline-stage completed"
                                        } else {
                                            "pipeline-stage"
                                        };
                                        let label = match *stage {
                                            "plan" => "Plan",
                                            "code" => "Code",
                                            "qa" => "QA",
                                            "done" => "Done",
                                            _ => stage,
                                        };
                                        view! {
                                            <span class={cls}>{label}</span>
                                        }
                                    }).collect::<Vec<_>>();

                                    let tags_view = bead.tags.iter().map(|tag| {
                                        let tag_class = match tag.as_str() {
                                            "High" => "tag tag-high",
                                            "Stuck" => "tag tag-stuck",
                                            "Needs Recovery" => "tag tag-recovery",
                                            "PR Created" => "tag tag-pr-created",
                                            "Feature" => "tag tag-feature",
                                            "Refactoring" => "tag tag-refactoring",
                                            "Incomplete" => "tag tag-incomplete",
                                            "Needs Resume" => "tag tag-needs-resume",
                                            _ => "tag",
                                        };
                                        view! {
                                            <span class={tag_class}>{tag.clone()}</span>
                                        }
                                    }).collect::<Vec<_>>();

                                    let agents_view = bead.agent_names.iter().enumerate().map(|(i, name)| {
                                        let colors = ["#7c3aed", "#c026d3", "#06b6d4", "#22c55e", "#eab308", "#ef4444"];
                                        let color = colors[i % colors.len()];
                                        let initial = name.chars().next().unwrap_or('?').to_string();
                                        view! {
                                            <span
                                                class="agent-dot"
                                                title={name.clone()}
                                                style={format!("background: {}; color: white;", color)}
                                            >
                                                {initial}
                                            </span>
                                        }
                                    }).collect::<Vec<_>>();

                                    // Action button with actual click handler
                                    let action_view = bead.action.as_ref().map(|action| {
                                        let action_str = action.clone();
                                        let bead_id_action = bead_id.clone();
                                        let bead_lane = bead.lane.clone();
                                        let move_bead_click = move_bead_action.clone();
                                        let btn_class = match action.as_str() {
                                            "start" => "action-btn action-start",
                                            "recover" => "action-btn action-recover",
                                            "resume" => "action-btn action-resume",
                                            _ => "action-btn",
                                        };
                                        let label = match action.as_str() {
                                            "start" => "Start",
                                            "recover" => "Recover",
                                            "resume" => "Resume",
                                            _ => "Action",
                                        };
                                        view! {
                                            <button
                                                class={btn_class}
                                                on:click=move |ev| {
                                                    ev.stop_propagation();
                                                    if let Some(target) = next_lane_for_action(&action_str, &bead_lane) {
                                                        move_bead_click(bead_id_action.clone(), target);
                                                    }
                                                }
                                            >
                                                {label}
                                            </button>
                                        }
                                    });

                                    // Forward button (move to next column)
                                    let bead_id_fwd = bead_id.clone();
                                    let bead_lane_fwd = bead.lane.clone();
                                    let move_bead_fwd = move_bead_action.clone();
                                    let show_forward = bead.lane != Lane::PrCreated;
                                    let forward_view = show_forward.then(|| {
                                        view! {
                                            <button
                                                class="action-btn action-forward"
                                                title="Move to next stage"
                                                on:click=move |ev| {
                                                    ev.stop_propagation();
                                                    if let Some(target) = next_lane_for_action("forward", &bead_lane_fwd) {
                                                        move_bead_fwd(bead_id_fwd.clone(), target);
                                                    }
                                                }
                                            >
                                                "\u{2192}"
                                            </button>
                                        }
                                    });

                                    // Drag start handler
                                    let on_dragstart = move |ev: DragEvent| {
                                        if let Some(dt) = ev.data_transfer() {
                                            let _ = dt.set_data("text/plain", &bead_id_drag);
                                            let _ = dt.set_drop_effect("move");
                                        }
                                        set_dragging_start.set(Some(bead_id_drag.clone()));
                                    };

                                    let on_dragend = move |_ev: DragEvent| {
                                        set_dragging_end.set(None);
                                    };

                                    view! {
                                        <div
                                            class={format!("bead-card {}", status_class)}
                                            draggable="true"
                                            on:dragstart=on_dragstart
                                            on:dragend=on_dragend
                                        >
                                            <div class="bead-card-header">
                                                <span class="bead-title">{bead.title.clone()}</span>
                                                <div class="bead-card-actions">
                                                    {forward_view}
                                                </div>
                                            </div>
                                            <div class="bead-id">{bead.id.clone()}</div>
                                            <div class="bead-description">{bead.description.clone()}</div>
                                            <div class="bead-tags">{tags_view}</div>
                                            <div class="progress-pipeline">{pipeline_view}</div>
                                            <div class="bead-footer">
                                                <div class="bead-agents">{agents_view}</div>
                                                <span class="bead-timestamp">{bead.timestamp.clone()}</span>
                                                {action_view}
                                            </div>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()
                        }}
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}
