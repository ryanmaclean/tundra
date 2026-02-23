use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::DragEvent;

use crate::api::ApiBead;
use crate::i18n::t;
use crate::state::use_app_state;
use crate::themed::{themed, Prompt};
use crate::types::{BeadResponse, BeadStatus, Lane};

/// Map an action to the next lane the bead should move to.
fn next_lane_for_action(action: &str, current: &Lane) -> Option<Lane> {
    match action {
        "start" => Some(Lane::InProgress),
        "recover" => Some(Lane::Planning),
        "resume" => Some(Lane::InProgress),
        "review" => Some(Lane::AiReview),
        "approve" => Some(Lane::Done),
        "reject" => Some(Lane::InProgress),
        // Generic forward movement (5 display columns: Planning → In Progress → AI Review → Human Review → Done)
        "forward" => match current {
            Lane::Backlog | Lane::Queue | Lane::Planning => Some(Lane::InProgress),
            Lane::InProgress => Some(Lane::AiReview),
            Lane::AiReview => Some(Lane::HumanReview),
            Lane::HumanReview => Some(Lane::Done),
            Lane::Done | Lane::PrCreated => None,
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

/// Map a backend API status string to a Lane.
fn lane_from_api_status(status: &str) -> Lane {
    match status {
        "backlog" => Lane::Backlog,
        "hooked" => Lane::Queue,
        "slung" => Lane::InProgress,
        "review" => Lane::AiReview,
        "done" => Lane::Done,
        "failed" => Lane::Backlog,
        "escalated" => Lane::HumanReview,
        _ => Lane::Backlog,
    }
}

/// Map a Lane to a backend API status string.
fn api_status_from_lane(lane: &Lane) -> &'static str {
    match lane {
        Lane::Backlog => "backlog",
        Lane::Queue => "hooked",
        Lane::Planning => "backlog",
        Lane::InProgress => "slung",
        Lane::AiReview => "review",
        Lane::HumanReview => "escalated",
        Lane::Done => "done",
        Lane::PrCreated => "done",
    }
}

/// Convert an ApiBead from the API into the BeadResponse used by the UI.
pub fn api_bead_to_bead_response(ab: &ApiBead) -> BeadResponse {
    let lane = lane_from_api_status(&ab.status);
    let status = status_for_lane(&lane);
    let progress = progress_for_lane(&lane);
    let action = action_for_lane(&lane);

    // Populate tags from category and priority fields so filters work
    let mut tags = vec![];
    if let Some(ref cat) = ab.category {
        if !cat.is_empty() {
            tags.push(cat.clone());
        }
    }
    if let Some(ref pri) = ab.priority_label {
        if !pri.is_empty() {
            tags.push(pri.clone());
        }
    } else {
        // Fall back to mapping the integer priority to a label
        let pri_label = match ab.priority {
            4 => "Critical",
            3 => "High",
            2 => "Medium",
            1 => "Low",
            _ => "",
        };
        if !pri_label.is_empty() {
            tags.push(pri_label.to_string());
        }
    }

    // Derive subtask statuses from progress stage as a visual approximation.
    // When real subtask data flows through the API this will use actual values.
    let subtask_statuses = match progress.as_str() {
        "plan" => vec!["in_progress", "pending", "pending"],
        "code" => vec!["complete", "in_progress", "pending", "pending"],
        "qa" => vec!["complete", "complete", "in_progress", "pending"],
        "done" => vec!["complete", "complete", "complete", "complete"],
        _ => vec!["pending", "pending"],
    }.into_iter().map(String::from).collect();

    BeadResponse {
        id: ab.id.clone(),
        title: ab.title.clone(),
        status,
        lane,
        agent_id: None,
        description: ab.description.clone().unwrap_or_default(),
        tags,
        progress_stage: progress,
        agent_names: vec![],
        timestamp: String::new(),
        action,
        subtask_statuses,
    }
}

/// Short initials for known agent roles/names.
pub fn agent_initials(name: &str) -> String {
    let n = name.to_lowercase();
    if n.contains("crew") {
        "CR".to_string()
    } else if n.contains("swarm") {
        "SW".to_string()
    } else if n.contains("planner") {
        "PL".to_string()
    } else if n.contains("coder") {
        "CD".to_string()
    } else if n.contains("review") {
        "RV".to_string()
    } else if n.contains("test") {
        "TS".to_string()
    } else if n.contains("debug") {
        "DB".to_string()
    } else if n.contains("architect") {
        "AR".to_string()
    } else {
        let mut chars = name.chars();
        let first = chars.next().map(|c| c.to_ascii_uppercase()).unwrap_or('X');
        let second = chars.next().map(|c| c.to_ascii_uppercase());
        match second {
            Some(s) => format!("{}{}", first, s),
            None => first.to_string(),
        }
    }
}

/// Progress lane class for compact P/C/Q/D pills.
pub fn stage_class(progress: &str, stage: &str) -> &'static str {
    if progress == "done" {
        if stage == "done" {
            "stage active"
        } else {
            "stage completed"
        }
    } else {
        let order = |s: &str| match s {
            "plan" => 0,
            "code" => 1,
            "qa" => 2,
            "done" => 3,
            _ => -1,
        };
        let p = order(progress);
        let st = order(stage);
        if st < p {
            "stage completed"
        } else if st == p {
            "stage active"
        } else {
            "stage pending"
        }
    }
}

/// Tag class mapper used by Kanban chips.
pub fn bead_tag_class(tag: &str) -> &'static str {
    let t = tag.to_lowercase();
    if t.contains("stuck") || t.contains("recovery") || t.contains("needs recovery") {
        "bead-tag bead-tag-recovery"
    } else if t.contains("critical") || t.contains("urgent") {
        "bead-tag bead-tag-critical"
    } else if t.contains("high") || t.contains("medium") || t.contains("low") {
        "bead-tag bead-tag-priority"
    } else if t.contains("feature")
        || t.contains("bug")
        || t.contains("refactor")
        || t.contains("infra")
        || t.contains("security")
        || t.contains("performance")
        || t.contains("ui")
        || t.contains("test")
        || t.contains("doc")
    {
        "bead-tag bead-tag-category"
    } else if t.contains("incomplete")
        || t.contains("blocked")
        || t.contains("pending")
        || t.contains("interrupted")
        || t.contains("status")
    {
        "bead-tag bead-tag-status"
    } else {
        "bead-tag bead-tag-priority"
    }
}

/// Phase status class used beside the per-card phase line.
pub fn phase_status_class(bead: &BeadResponse) -> &'static str {
    let needs_warn = bead.tags.iter().any(|t| {
        let v = t.to_lowercase();
        v.contains("stuck") || v.contains("recovery") || v.contains("needs recovery")
    });
    if needs_warn {
        return "bead-phase-status bead-phase-interrupted";
    }
    match bead.status {
        BeadStatus::Planning => "bead-phase-status bead-phase-planning",
        BeadStatus::InProgress => "bead-phase-status bead-phase-progress",
        BeadStatus::AiReview => "bead-phase-status bead-phase-review",
        BeadStatus::HumanReview => "bead-phase-status bead-phase-review",
        BeadStatus::Done => "bead-phase-status bead-phase-complete",
        BeadStatus::Failed => "bead-phase-status bead-phase-interrupted",
    }
}

/// SVG icon markup for the phase indicator.
pub fn phase_status_icon_svg(bead: &BeadResponse) -> String {
    let needs_warn = bead.tags.iter().any(|t| {
        let v = t.to_lowercase();
        v.contains("stuck") || v.contains("recovery") || v.contains("needs recovery")
    });
    if needs_warn {
        return r#"<svg width="14" height="14" viewBox="0 0 24 24" class="phase-icon phase-icon-warn" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 9v4"/><path d="M12 17h.01"/><path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/></svg>"#.to_string();
    }

    match bead.status {
        BeadStatus::Planning => r#"<svg width="14" height="14" viewBox="0 0 24 24" class="phase-icon phase-icon-plan" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m12 3 8 9-8 9-8-9 8-9z"/></svg>"#.to_string(),
        BeadStatus::InProgress => r#"<svg width="14" height="14" viewBox="0 0 24 24" class="phase-icon phase-icon-active" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="8 5 19 12 8 19 8 5"/></svg>"#.to_string(),
        BeadStatus::AiReview | BeadStatus::HumanReview => r#"<svg width="14" height="14" viewBox="0 0 24 24" class="phase-icon phase-icon-review" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7z"/><circle cx="12" cy="12" r="3"/></svg>"#.to_string(),
        BeadStatus::Done => r#"<svg width="14" height="14" viewBox="0 0 24 24" class="phase-icon phase-icon-done" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12" stroke-dasharray="24" stroke-dashoffset="24"><animate attributeName="stroke-dashoffset" from="24" to="0" dur="0.5s" begin="0s" fill="freeze"/></polyline></svg>"#.to_string(),
        BeadStatus::Failed => r#"<svg width="14" height="14" viewBox="0 0 24 24" class="phase-icon phase-icon-fail" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>"#.to_string(),
    }
}

/// Lane index for collapse state tracking (5 display columns).
fn lane_index(lane: &Lane) -> usize {
    match lane {
        Lane::Backlog | Lane::Queue | Lane::Planning => 0,
        Lane::InProgress => 1,
        Lane::AiReview => 2,
        Lane::HumanReview => 3,
        Lane::Done | Lane::PrCreated => 4,
    }
}

#[component]
pub fn BeadsPage() -> impl IntoView {
    let state = use_app_state();
    let beads = state.beads;
    let set_beads = state.set_beads;
    let set_dragging = state.set_dragging_bead;
    let dragging_bead = state.dragging_bead;
    let mode = state.display_mode;

    // Track which column is being dragged over for visual feedback
    let (drag_over_lane, set_drag_over_lane) = signal(Option::<usize>::None);

    // Fetch beads from API on mount
    {
        let set_beads = set_beads.clone();
        leptos::task::spawn_local(async move {
            match crate::api::fetch_beads().await {
                Ok(api_beads) => {
                    let ui_beads: Vec<BeadResponse> =
                        api_beads.iter().map(api_bead_to_bead_response).collect();
                    if !ui_beads.is_empty() {
                        set_beads.set(ui_beads);
                    }
                }
                Err(e) => {
                    leptos::logging::log!("Failed to fetch beads from API: {}", e);
                }
            }
        });
    }

    // Filter state
    let (filter_category, set_filter_category) = signal("All".to_string());
    let (filter_priority, set_filter_priority) = signal("All".to_string());
    let (filter_search, set_filter_search) = signal(String::new());

    // Task detail modal state
    let (selected_bead_id, set_selected_bead_id) = signal(Option::<String>::None);

    // New task modal state
    let (show_new_task_for_column, set_show_new_task_for_column) = signal(Option::<Lane>::None);

    // Column collapse state: a Vec<bool> of 5 display columns, all start expanded (false = not collapsed)
    let (collapsed, set_collapsed) = signal(vec![false; 5]);

    let on_add_task = move |target_lane: Lane| {
        set_show_new_task_for_column.set(Some(target_lane));
    };

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
        (Lane::Planning, "Planning", "column-border-planning"),
        (Lane::InProgress, "In Progress", "column-border-inprogress"),
        (Lane::AiReview, "AI Review", "column-border-aireview"),
        (Lane::HumanReview, "Human Review", "column-border-humanreview"),
        (Lane::Done, "Done", "column-border-done"),
    ];

    // Move a bead to a target lane (optimistic local update + API call with rollback)
    let move_bead = move |bead_id: String, target_lane: Lane| {
        // Snapshot for rollback
        let prev_beads = beads.get_untracked();

        // Optimistic local update
        set_beads.update(|beads_vec| {
            if let Some(bead) = beads_vec.iter_mut().find(|b| b.id == bead_id) {
                bead.lane = target_lane.clone();
                bead.status = status_for_lane(&target_lane);
                bead.progress_stage = progress_for_lane(&target_lane);
                bead.action = action_for_lane(&target_lane);
                bead.timestamp = "just now".to_string();
            }
        });

        // Persist to API
        let api_status = api_status_from_lane(&target_lane).to_string();
        let id = bead_id.clone();
        leptos::task::spawn_local(async move {
            if let Err(e) = crate::api::update_bead_status(&id, &api_status).await {
                // Rollback optimistic update
                set_beads.set(prev_beads);
                leptos::logging::log!("Failed to update bead status via API (rolled back): {}", e);
            }
        });
    };

    // Auto-refresh interval state
    let (auto_refresh_secs, set_auto_refresh_secs) = signal(0u32); // 0 = off

    // Refresh handler
    let do_refresh = {
        let set_beads = set_beads.clone();
        move || {
            let set_beads = set_beads.clone();
            leptos::task::spawn_local(async move {
                match crate::api::fetch_beads().await {
                    Ok(api_beads) => {
                        let ui_beads: Vec<BeadResponse> =
                            api_beads.iter().map(api_bead_to_bead_response).collect();
                        set_beads.set(ui_beads);
                    }
                    Err(e) => {
                        leptos::logging::log!("Failed to refresh beads: {}", e);
                    }
                }
            });
        }
    };

    // Auto-refresh interval timer
    {
        let do_refresh_interval = do_refresh.clone();
        Effect::new(move |prev_handle: Option<Option<i32>>| {
            // Clear any previous interval
            if let Some(Some(handle)) = prev_handle {
                if let Some(window) = web_sys::window() {
                    window.clear_interval_with_handle(handle);
                }
            }
            let secs = auto_refresh_secs.get();
            if secs == 0 {
                return None;
            }
            let refresh = do_refresh_interval.clone();
            let cb = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                refresh();
            }) as Box<dyn FnMut()>);
            let handle = web_sys::window().and_then(|w| {
                w.set_interval_with_callback_and_timeout_and_arguments_0(
                    cb.as_ref().unchecked_ref(),
                    (secs * 1000) as i32,
                )
                .ok()
            });
            cb.forget();
            handle
        });
    }

    let do_refresh_click = do_refresh.clone();

    view! {
        <div class="page-header">
            <h2>{t("kanban-title")}</h2>
            <div class="page-header-actions">
                <button class="refresh-btn" on:click=move |_| do_refresh_click()>
                    "\u{21BB} Refresh"
                </button>
                <select
                    class="interval-select"
                    prop:value=move || auto_refresh_secs.get().to_string()
                    on:change=move |ev| {
                        let val: u32 = event_target_value(&ev).parse().unwrap_or(0);
                        set_auto_refresh_secs.set(val);
                    }
                >
                    <option value="0">"Interval: Off"</option>
                    <option value="5">"5s"</option>
                    <option value="10">"10s"</option>
                    <option value="30">"30s"</option>
                    <option value="60">"60s"</option>
                </select>
            </div>
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
            {lanes.into_iter().map(|(lane, label, dot_class)| {
                let lane_for_render = lane.clone();
                let lane_for_drop = lane.clone();
                let lane_for_over = lane.clone();
                let move_bead_drop = move_bead.clone();
                let col_idx = lane_index(&lane);

                let is_collapsed = move || {
                    collapsed.get().get(col_idx).copied().unwrap_or(false)
                };

                let toggle_collapse = move |_| {
                    set_collapsed.update(|cols| {
                        if let Some(v) = cols.get_mut(col_idx) {
                            *v = !*v;
                        }
                    });
                };

                let lane_for_filter = lane.clone();
                let count = move || {
                    beads.get().into_iter()
                        .filter(|b| match lane_for_filter {
                            Lane::Planning => matches!(b.lane, Lane::Backlog | Lane::Queue | Lane::Planning),
                            Lane::InProgress => matches!(b.lane, Lane::InProgress),
                            Lane::AiReview => matches!(b.lane, Lane::AiReview),
                            Lane::HumanReview => matches!(b.lane, Lane::HumanReview),
                            Lane::Done => matches!(b.lane, Lane::Done | Lane::PrCreated),
                            _ => false,
                        })
                        .count()
                };

                // Drag-and-drop: on_dragover -- allow drop & highlight column
                let on_dragover = move |ev: DragEvent| {
                    ev.prevent_default();
                    let _ = &lane_for_over; // keep in scope
                    set_drag_over_lane.set(Some(col_idx));
                };

                // Drag-and-drop: on_dragleave -- remove highlight
                let on_dragleave = move |_ev: DragEvent| {
                    // Only clear if this column is the current drag-over target
                    if drag_over_lane.get() == Some(col_idx) {
                        set_drag_over_lane.set(None);
                    }
                };

                // Drag-and-drop: on_drop -- move bead to this lane
                let on_drop = move |ev: DragEvent| {
                    ev.prevent_default();
                    if let Some(dt) = ev.data_transfer() {
                        if let Ok(bead_id) = dt.get_data("text/plain") {
                            if !bead_id.is_empty() {
                                // Map display lanes directly to technical lanes
                                let target_technical_lane = lane_for_drop.clone();
                                move_bead_drop(bead_id, target_technical_lane);
                            }
                        }
                    }
                    set_dragging.set(None);
                    set_drag_over_lane.set(None);
                };

                let col_class = move || {
                    let is_over = drag_over_lane.get() == Some(col_idx) && dragging_bead.get().is_some();
                    let base = match (is_collapsed(), is_over) {
                        (true, _) => "kanban-column kanban-column-collapsed",
                        (false, true) => "kanban-column drag-over drop-target",
                        (false, false) => "kanban-column drop-target",
                    };
                    format!("{} {}", base, dot_class)
                };

                let lane_for_add = lane.clone();
                let on_add_task_click = on_add_task.clone();

                view! {
                    <div
                        class=col_class
                        on:dragover=on_dragover
                        on:dragleave=on_dragleave
                        on:drop=on_drop
                    >
                        <h3>
                            <button
                                class="column-collapse-btn"
                                on:click=toggle_collapse
                                title=move || if is_collapsed() { "Expand column" } else { "Collapse column" }
                            >
                                {move || if is_collapsed() { "+" } else { "-" }}
                            </button>
                            {label}
                            " "
                            <span class="count">{count}</span>
                            <button
                                class="column-add-btn"
                                title="Add new task"
                                on:click=move |_| on_add_task_click(lane_for_add.clone())
                            >
                                "+"
                            </button>
                        </h3>
                        {move || {
                            if is_collapsed() {
                                return Vec::<AnyView>::new();
                            }
                            let move_bead_action = move_bead.clone();
                            let cat_filter = filter_category.get();
                            let pri_filter = filter_priority.get();
                            let search_filter = filter_search.get().to_lowercase();

                            let category_skip = ["Critical", "High", "Medium", "Low", "Stuck", "Needs Recovery", "PR Created", "Incomplete", "Needs Resume"];
                            let priority_values = ["Critical", "High", "Medium", "Low"];

                            let filtered: Vec<BeadResponse> = beads.get().into_iter()
                                .filter(|b| match lane_for_render {
                                    Lane::Planning => matches!(b.lane, Lane::Backlog | Lane::Queue | Lane::Planning),
                                    Lane::InProgress => matches!(b.lane, Lane::InProgress),
                                    Lane::AiReview => matches!(b.lane, Lane::AiReview),
                                    Lane::HumanReview => matches!(b.lane, Lane::HumanReview),
                                    Lane::Done => matches!(b.lane, Lane::Done | Lane::PrCreated),
                                    _ => false,
                                })
                                .filter(|b| {
                                    if cat_filter == "All" { return true; }
                                    // Match category: first tag that is NOT a priority/status keyword
                                    b.tags.iter().any(|t| !category_skip.contains(&t.as_str()) && *t == cat_filter)
                                })
                                .filter(|b| {
                                    if pri_filter == "All" { return true; }
                                    // Match priority: first tag that IS a priority keyword
                                    b.tags.iter().any(|t| priority_values.contains(&t.as_str()) && *t == pri_filter)
                                })
                                .filter(|b| {
                                    if search_filter.is_empty() { return true; }
                                    b.title.to_lowercase().contains(&search_filter)
                                })
                                .collect();

                            if filtered.is_empty() && lane_for_render == Lane::Planning {
                                return vec![view! {
                                    <div class="kanban-empty-state">
                                        {themed(mode.get(), Prompt::EmptyBacklog)}
                                    </div>
                                }.into_any()];
                            }

                            filtered.into_iter()
                                .map(|bead| {
                                    let bead_id = bead.id.clone();
                                    let bead_id_drag = bead.id.clone();

                                    let status_class = match bead.status {
                                        BeadStatus::Planning => "status-planning",
                                        BeadStatus::InProgress => "status-in-progress",
                                        BeadStatus::AiReview => "status-ai-review",
                                        BeadStatus::HumanReview => "status-human-review",
                                        BeadStatus::Done => "status-done",
                                        BeadStatus::Failed => "status-failed",
                                    };

                                    // Phase segment classes for the segmented progress bar
                                    // Each segment: planning (20%), coding (60%), QA (15%), done (5%)
                                    let (seg_plan, seg_code, seg_qa, seg_done) = match bead.progress_stage.as_str() {
                                        "plan" => ("phase-seg phase-seg-plan active", "phase-seg phase-seg-code", "phase-seg phase-seg-qa", "phase-seg phase-seg-done"),
                                        "code" => ("phase-seg phase-seg-plan filled", "phase-seg phase-seg-code active", "phase-seg phase-seg-qa", "phase-seg phase-seg-done"),
                                        "qa"   => ("phase-seg phase-seg-plan filled", "phase-seg phase-seg-code filled", "phase-seg phase-seg-qa active", "phase-seg phase-seg-done"),
                                        "done" => ("phase-seg phase-seg-plan filled", "phase-seg phase-seg-code filled", "phase-seg phase-seg-qa filled", "phase-seg phase-seg-done filled"),
                                        _      => ("phase-seg phase-seg-plan", "phase-seg phase-seg-code", "phase-seg phase-seg-qa", "phase-seg phase-seg-done"),
                                    };

                                    let (plan_cls, code_cls, qa_cls) = match bead.progress_stage.as_str() {
                                        "plan" => ("pipeline-stage active", "pipeline-stage", "pipeline-stage"),
                                        "code" => ("pipeline-stage completed", "pipeline-stage active", "pipeline-stage"),
                                        "qa" => ("pipeline-stage completed", "pipeline-stage completed", "pipeline-stage active"),
                                        "done" => ("pipeline-stage completed", "pipeline-stage completed", "pipeline-stage completed"),
                                        _ => ("pipeline-stage", "pipeline-stage", "pipeline-stage"),
                                    };

                                    // Priority badge
                                    let priority_badge = bead.tags.iter().find(|t| matches!(t.as_str(), "Critical" | "High" | "Medium" | "Low")).cloned();
                                    let priority_view = priority_badge.map(|p| {
                                        let cls = match p.as_str() {
                                            "Critical" => "card-badge badge-critical",
                                            "High" => "card-badge badge-high",
                                            "Medium" => "card-badge badge-medium",
                                            "Low" => "card-badge badge-low",
                                            _ => "card-badge badge-medium",
                                        };
                                        view! { <span class={cls}>{p}</span> }
                                    });

                                    // Category badge
                                    let category_skip = ["Critical", "High", "Medium", "Low", "Stuck", "Needs Recovery", "PR Created", "Incomplete", "Needs Resume"];
                                    let category_badge = bead.tags.iter().find(|t| !category_skip.contains(&t.as_str())).cloned();
                                    let category_view = category_badge.map(|c| {
                                        let cls = match c.as_str() {
                                            "Feature" => "card-badge badge-feature",
                                            "Refactoring" => "card-badge badge-refactor",
                                            "Bug" | "Bug Fix" => "card-badge badge-bug",
                                            "Security" => "card-badge badge-security",
                                            _ => "card-badge badge-default",
                                        };
                                        view! { <span class={cls}>{c}</span> }
                                    });

                                    // Agent indicator
                                    let has_agent = !bead.agent_names.is_empty();

                                    // Subtask dots
                                    let subtask_dots: Vec<_> = bead.subtask_statuses.iter().map(|s| {
                                        let dot_cls = match s.as_str() {
                                            "complete" => "subtask-dot subtask-complete",
                                            "in_progress" => "subtask-dot subtask-active",
                                            "failed" => "subtask-dot subtask-failed",
                                            _ => "subtask-dot subtask-pending",
                                        };
                                        view! { <span class={dot_cls}></span> }
                                    }).collect();

                                    let _tags_view = bead.tags.iter().map(|tag| {
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
                                            "start" => "\u{25B6} Start",
                                            "recover" => "\u{21BB} Recover",
                                            "resume" => "\u{23EF} Resume",
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
                                        set_dragging.set(Some(bead_id_drag.clone()));
                                    };

                                    let on_dragend = move |_ev: DragEvent| {
                                        set_dragging.set(None);
                                    };

                                    let bead_id_for_class = bead_id.clone();

                                    // Click handler to open task detail
                                    let on_card_click = move |_| {
                                        set_selected_bead_id.set(Some(bead_id.clone()));
                                    };
                                    let card_class = move || {
                                        let is_dragging = dragging_bead.get().as_deref() == Some(bead_id_for_class.as_str());
                                        if is_dragging {
                                            format!("bead-card {} dragging", status_class)
                                        } else {
                                            format!("bead-card {}", status_class)
                                        }
                                    };

                                    view! {
                                        <div
                                            class=card_class
                                            draggable="true"
                                            on:dragstart=on_dragstart
                                            on:dragend=on_dragend
                                            on:click=on_card_click
                                        >
                                            <div class="bead-card-header">
                                                <span class="bead-title">{bead.title.clone()}</span>
                                                <div class="bead-card-actions">
                                                    {forward_view}
                                                </div>
                                            </div>
                                            <div class="bead-description">{bead.description.clone()}</div>
                                            // Badges row: priority + category
                                            <div class="bead-badges">
                                                {priority_view}
                                                {category_view}
                                                // Agent indicator
                                                {has_agent.then(|| {
                                                    let agent_label = bead.agent_names.first().cloned().unwrap_or_default();
                                                    let initial = agent_label.chars().next().unwrap_or('?').to_uppercase().to_string();
                                                    view! {
                                                        <span class="card-badge badge-agent" title={agent_label}>
                                                            {initial}
                                                        </span>
                                                    }
                                                })}
                                            </div>
                                            // Phase segments progress bar (Auto Claude style)
                                            <div class="phase-segments-bar">
                                                <div class={seg_plan} title="Planning"></div>
                                                <div class={seg_code} title="Coding"></div>
                                                <div class={seg_qa} title="QA Review"></div>
                                                <div class={seg_done} title="Complete"></div>
                                            </div>
                                            // Pipeline labels below the bar
                                            <div class="progress-pipeline">
                                                <span class={plan_cls}>
                                                    {"P"}
                                                </span>
                                                <span class={code_cls}>
                                                    {"C"}
                                                </span>
                                                <span class={qa_cls}>
                                                    {"Q"}
                                                </span>
                                            </div>
                                            <div class="subtask-dots">
                                                {subtask_dots}
                                            </div>
                                            <div class="bead-footer">
                                                <div class="bead-agents">{agents_view}</div>
                                                <span class="bead-timestamp">
                                                    {if bead.timestamp.is_empty() { "just now".to_string() } else { bead.timestamp.clone() }}
                                                </span>
                                                {action_view}
                                            </div>
                                        </div>
                                    }.into_any()
                                }).collect::<Vec<_>>()
                        }}
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>

        // Task detail modal
        {move || selected_bead_id.get().map(|id| {
            view! {
                <crate::components::task_detail::TaskDetail
                    bead_id=id
                    on_close=move |_| set_selected_bead_id.set(None)
                />
            }
        })}

        // New task modal for column-specific creation
        {move || show_new_task_for_column.get().map(|target_lane| {
            view! {
                <crate::components::new_task_modal::NewTaskModal
                    target_lane=target_lane
                    on_close=move |_| set_show_new_task_for_column.set(None)
                />
            }
        })}
    }
}
