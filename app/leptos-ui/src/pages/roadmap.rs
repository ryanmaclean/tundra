use leptos::prelude::*;
use crate::state::use_app_state;
use crate::themed::{themed, Prompt};
use leptos::task::spawn_local;
use web_sys::DragEvent;

use crate::api;
use crate::i18n::t;

#[component]
pub fn RoadmapPage() -> impl IntoView {
    let app_state = use_app_state();
    let display_mode = app_state.display_mode;
    let (features, set_features) = signal(Vec::<api::ApiRoadmapItem>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (expanded_id, set_expanded_id) = signal(Option::<String>::None);
    let (generating, set_generating) = signal(false);
    let (show_add_form, set_show_add_form) = signal(false);
    let (new_title, set_new_title) = signal(String::new());
    let (new_desc, set_new_desc) = signal(String::new());
    let (new_priority, set_new_priority) = signal("Medium".to_string());

    // Task creation feedback
    let (task_created_msg, set_task_created_msg) = signal(Option::<String>::None);

    // View mode: "kanban", "all", "priority"
    let (view_mode, set_view_mode) = signal("kanban".to_string());

    // Drag-and-drop state
    let (dragging_id, set_dragging_id) = signal(Option::<String>::None);
    let (drag_over_col, set_drag_over_col) = signal(Option::<usize>::None);

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::fetch_roadmap().await {
                Ok(data) => set_features.set(data),
                Err(e) => set_error_msg.set(Some(format!("Failed to fetch roadmap: {e}"))),
            }
            set_loading.set(false);
        });
    };

    do_refresh();

    let on_generate = move |_| {
        set_generating.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::generate_roadmap().await {
                Ok(data) => set_features.set(data),
                Err(e) => set_error_msg.set(Some(format!("Failed to generate roadmap: {e}"))),
            }
            set_generating.set(false);
        });
    };

    let (adding, set_adding) = signal(false);

    let on_add_feature = move |_| {
        let title = new_title.get();
        let desc = new_desc.get();
        let priority = new_priority.get();
        if title.trim().is_empty() {
            return;
        }
        set_adding.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::add_roadmap_feature(&title, &desc, "proposed", &priority.to_lowercase()).await {
                Ok(feature) => {
                    set_features.update(|f| {
                        f.push(feature);
                        if f.len() > 100 {
                            f.drain(..f.len() - 100);
                        }
                    });
                    set_new_title.set(String::new());
                    set_new_desc.set(String::new());
                    set_new_priority.set("Medium".to_string());
                    set_show_add_form.set(false);
                }
                Err(e) => {
                    let feature = api::ApiRoadmapItem {
                        id: format!("feat-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("000")),
                        title,
                        description: desc,
                        status: "Planned".to_string(),
                        priority,
                    };
                    set_features.update(|f| {
                        f.push(feature);
                        if f.len() > 100 {
                            f.drain(..f.len() - 100);
                        }
                    });
                    set_new_title.set(String::new());
                    set_new_desc.set(String::new());
                    set_new_priority.set("Medium".to_string());
                    set_show_add_form.set(false);
                    web_sys::console::warn_1(&format!("API fallback (local add): {e}").into());
                }
            }
            set_adding.set(false);
        });
    };

    let status_class = |status: &str| -> &'static str {
        match status {
            "Planned" | "planned" | "proposed" => "roadmap-badge-planned",
            "InProgress" | "in_progress" | "In Progress" => "roadmap-badge-inprogress",
            "Done" | "done" | "completed" => "roadmap-badge-done",
            "Under Review" | "under_review" | "review" => "roadmap-badge-review",
            _ => "roadmap-badge-planned",
        }
    };

    let priority_class = |priority: &str| -> &'static str {
        match priority {
            "High" | "high" => "roadmap-priority-high",
            "Medium" | "medium" => "roadmap-priority-medium",
            "Low" | "low" => "roadmap-priority-low",
            _ => "roadmap-priority-medium",
        }
    };

    // Kanban columns for roadmap
    let kanban_columns = vec![
        (t("roadmap-columns-review"), vec!["Under Review", "under_review", "review", "proposed"]),
        (t("roadmap-columns-planned"), vec!["Planned", "planned"]),
        (t("roadmap-columns-progress"), vec!["InProgress", "in_progress", "In Progress"]),
        (t("roadmap-columns-done"), vec!["Done", "done", "completed"]),
    ];

    view! {
        <div class="page-header">
            <h2>{t("roadmap-title")}</h2>
            <div class="page-header-actions">
                <button
                    class="action-btn action-start"
                    on:click=on_generate
                    disabled=move || generating.get()
                >
                    {move || if generating.get() { "Generating...".to_string() } else { t("roadmap-generate") }}
                </button>
                <button
                    class="action-btn action-forward"
                    on:click=move |_| set_show_add_form.set(!show_add_form.get())
                >
                    {format!("+ {}", t("roadmap-add-feature"))}
                </button>
                <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                    "Refresh"
                </button>
            </div>
        </div>

        // View mode toggles
        <div class="roadmap-view-toggles">
            <button
                class=move || if view_mode.get() == "kanban" { "view-toggle active" } else { "view-toggle" }
                on:click=move |_| set_view_mode.set("kanban".to_string())
            >"Kanban"</button>
            <button
                class=move || if view_mode.get() == "all" { "view-toggle active" } else { "view-toggle" }
                on:click=move |_| set_view_mode.set("all".to_string())
            >"All Features"</button>
            <button
                class=move || if view_mode.get() == "priority" { "view-toggle active" } else { "view-toggle" }
                on:click=move |_| set_view_mode.set("priority".to_string())
            >"By Priority"</button>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error">{msg}</div>
        })}

        {move || task_created_msg.get().map(|msg| view! {
            <div class="dashboard-success" style="color: #3fb950; background: #0d1117; border: 1px solid #238636; border-radius: 6px; padding: 8px 12px; margin-bottom: 8px;">
                {msg}
            </div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">{move || themed(display_mode.get(), Prompt::Loading)}</div>
        })}

        // Add Feature form
        {move || show_add_form.get().then(|| view! {
            <div class="roadmap-add-form">
                <h3>"Add New Feature"</h3>
                <div class="roadmap-form-fields">
                    <input
                        type="text"
                        class="form-input"
                        placeholder="Feature title..."
                        prop:value=move || new_title.get()
                        on:input=move |ev| set_new_title.set(event_target_value(&ev))
                    />
                    <textarea
                        class="form-textarea"
                        placeholder="Description..."
                        prop:value=move || new_desc.get()
                        on:input=move |ev| set_new_desc.set(event_target_value(&ev))
                    ></textarea>
                    <select
                        class="settings-select"
                        prop:value=move || new_priority.get()
                        on:change=move |ev| set_new_priority.set(event_target_value(&ev))
                    >
                        <option value="High">"High"</option>
                        <option value="Medium">"Medium"</option>
                        <option value="Low">"Low"</option>
                    </select>
                    <button
                        class="action-btn action-start"
                        on:click=on_add_feature
                        disabled=move || adding.get()
                    >
                        {move || if adding.get() { "Adding..." } else { "Add Feature" }}
                    </button>
                </div>
            </div>
        })}

        // Kanban view with drag-and-drop
        {move || (view_mode.get() == "kanban").then(|| {
            let columns = kanban_columns.clone();
            let all_features = features.get();
            view! {
                <div class="roadmap-kanban">
                    {columns.into_iter().enumerate().map(|(col_idx, (col_label, statuses))| {
                        let col_features: Vec<_> = all_features.iter()
                            .filter(|f| statuses.contains(&f.status.as_str()))
                            .cloned()
                            .collect();
                        let col_count = col_features.len();
                        let target_status = statuses[0].to_string();

                        let on_dragover = move |ev: DragEvent| {
                            ev.prevent_default();
                            set_drag_over_col.set(Some(col_idx));
                        };

                        let on_dragleave = move |_ev: DragEvent| {
                            if drag_over_col.get() == Some(col_idx) {
                                set_drag_over_col.set(None);
                            }
                        };

                        let target_status_drop = target_status.clone();
                        let on_drop = move |ev: DragEvent| {
                            ev.prevent_default();
                            if let Some(dt) = ev.data_transfer() {
                                if let Ok(feature_id) = dt.get_data("text/plain") {
                                    if !feature_id.is_empty() {
                                        let new_status = target_status_drop.clone();
                                        // Snapshot for rollback
                                        let prev_features = features.get_untracked();
                                        // Optimistic local update
                                        set_features.update(|fs| {
                                            if let Some(f) = fs.iter_mut().find(|f| f.id == feature_id) {
                                                f.status = new_status.clone();
                                            }
                                        });
                                        // Persist to API
                                        let fid = feature_id.clone();
                                        let st = new_status.clone();
                                        spawn_local(async move {
                                            if let Err(e) = api::update_roadmap_feature_status(&fid, &st).await {
                                                // Rollback optimistic update
                                                set_features.set(prev_features);
                                                set_error_msg.set(Some(format!("Failed to update feature status: {e}")));
                                            }
                                        });
                                    }
                                }
                            }
                            set_dragging_id.set(None);
                            set_drag_over_col.set(None);
                        };

                        let col_class = move || {
                            let is_over = drag_over_col.get() == Some(col_idx) && dragging_id.get().is_some();
                            if is_over {
                                "roadmap-kanban-column drag-over drop-target".to_string()
                            } else {
                                "roadmap-kanban-column drop-target".to_string()
                            }
                        };

                        view! {
                            <div
                                class=col_class
                                on:dragover=on_dragover
                                on:dragleave=on_dragleave
                                on:drop=on_drop
                            >
                                <h3 class="roadmap-kanban-column-header">
                                    {col_label}
                                    " "
                                    <span class="count">{col_count}</span>
                                </h3>
                                {col_features.into_iter().map(|feature| {
                                    let fid_click = feature.id.clone();
                                    let fid_drag = feature.id.clone();
                                    let fid_class = feature.id.clone();
                                    let title = feature.title.clone();
                                    let title_for_task = feature.title.clone();
                                    let desc_for_task = feature.description.clone();
                                    let desc_snippet = if feature.description.len() > 80 {
                                        format!("{}...", &feature.description[..80])
                                    } else {
                                        feature.description.clone()
                                    };
                                    let priority = feature.priority.clone();
                                    let pcls = priority_class(&priority);

                                    let on_dragstart = move |ev: DragEvent| {
                                        if let Some(dt) = ev.data_transfer() {
                                            let _ = dt.set_data("text/plain", &fid_drag);
                                            let _ = dt.set_drop_effect("move");
                                        }
                                        set_dragging_id.set(Some(fid_drag.clone()));
                                    };

                                    let on_dragend = move |_ev: DragEvent| {
                                        set_dragging_id.set(None);
                                        set_drag_over_col.set(None);
                                    };

                                    let card_class = move || {
                                        let is_dragging = dragging_id.get().as_deref() == Some(fid_class.as_str());
                                        if is_dragging {
                                            "roadmap-feature-card dragging".to_string()
                                        } else {
                                            "roadmap-feature-card".to_string()
                                        }
                                    };

                                    view! {
                                        <div
                                            class=card_class
                                            draggable="true"
                                            on:dragstart=on_dragstart
                                            on:dragend=on_dragend
                                            on:click=move |_| {
                                                if expanded_id.get().as_deref() == Some(&fid_click) {
                                                    set_expanded_id.set(None);
                                                } else {
                                                    set_expanded_id.set(Some(fid_click.clone()));
                                                }
                                            }
                                        >
                                            <div class="roadmap-feature-header">
                                                <span class="roadmap-feature-title">{title}</span>
                                            </div>
                                            <div class="roadmap-feature-desc">{desc_snippet}</div>
                                            <div class="roadmap-feature-meta">
                                                <span class={format!("roadmap-priority-badge {}", pcls)}>
                                                    {priority}
                                                </span>
                                                <button
                                                    class="btn btn-xs btn-outline roadmap-task-btn"
                                                    on:click={
                                                        let t = title_for_task.clone();
                                                        let d = desc_for_task.clone();
                                                        move |ev: web_sys::MouseEvent| {
                                                            ev.stop_propagation();
                                                            let t = t.clone();
                                                            let d = d.clone();
                                                            set_task_created_msg.set(None);
                                                            set_error_msg.set(None);
                                                            spawn_local(async move {
                                                                let desc = if d.is_empty() { None } else { Some(d.as_str()) };
                                                                match api::create_bead(&t, desc, Some("standard")).await {
                                                                    Ok(_bead) => {
                                                                        set_task_created_msg.set(Some(format!("Task created from '{}'", t)));
                                                                    }
                                                                    Err(e) => {
                                                                        set_error_msg.set(Some(format!("Failed to create task: {e}")));
                                                                    }
                                                                }
                                                            });
                                                        }
                                                    }
                                                >"Task"</button>
                                            </div>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                                {(col_count == 0).then(|| view! {
                                    <div class="roadmap-kanban-empty">"No features"</div>
                                })}
                            </div>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            }
        })}

        // All Features view (grid)
        {move || (view_mode.get() == "all").then(|| {
            view! {
                <div class="roadmap-grid">
                    {features.get().into_iter().map(|feature| {
                        let fid = feature.id.clone();
                        let fid_click = feature.id.clone();
                        let is_expanded = move || expanded_id.get().as_deref() == Some(&fid);
                        let scls = status_class(&feature.status);
                        let status_label = feature.status.clone();
                        let title = feature.title.clone();
                        let desc_snippet = if feature.description.len() > 100 {
                            format!("{}...", &feature.description[..100])
                        } else {
                            feature.description.clone()
                        };
                        let full_desc = feature.description.clone();
                        let priority = feature.priority.clone();
                        let pcls = priority_class(&priority);

                        view! {
                            <div
                                class="roadmap-feature-card"
                                on:click=move |_| {
                                    if expanded_id.get().as_deref() == Some(&fid_click) {
                                        set_expanded_id.set(None);
                                    } else {
                                        set_expanded_id.set(Some(fid_click.clone()));
                                    }
                                }
                            >
                                <div class="roadmap-feature-header">
                                    <span class="roadmap-feature-title">{title}</span>
                                    <span class={format!("roadmap-status-badge {}", scls)}>
                                        {status_label}
                                    </span>
                                </div>
                                <div class="roadmap-feature-meta">
                                    <span class={format!("roadmap-priority-badge {}", pcls)}>
                                        {priority}
                                    </span>
                                </div>
                                <div class="roadmap-feature-desc">
                                    {move || if is_expanded() { full_desc.clone() } else { desc_snippet.clone() }}
                                </div>
                            </div>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            }
        })}

        // By Priority view
        {move || (view_mode.get() == "priority").then(|| {
            let all_features = features.get();
            let high: Vec<_> = all_features.iter().filter(|f| matches!(f.priority.as_str(), "High" | "high")).cloned().collect();
            let medium: Vec<_> = all_features.iter().filter(|f| matches!(f.priority.as_str(), "Medium" | "medium")).cloned().collect();
            let low: Vec<_> = all_features.iter().filter(|f| matches!(f.priority.as_str(), "Low" | "low")).cloned().collect();
            let groups = vec![("High Priority", high), ("Medium Priority", medium), ("Low Priority", low)];
            view! {
                <div class="roadmap-priority-groups">
                    {groups.into_iter().map(|(label, items)| {
                        let item_count = items.len();
                        view! {
                            <div class="roadmap-priority-group">
                                <h3>{label} " " <span class="count">{item_count}</span></h3>
                                <div class="roadmap-grid">
                                    {items.into_iter().map(|feature| {
                                        let scls = status_class(&feature.status);
                                        let status_label = feature.status.clone();
                                        let title = feature.title.clone();
                                        let title_for_task = feature.title.clone();
                                        let desc_for_task = feature.description.clone();
                                        let desc_snippet = if feature.description.len() > 100 {
                                            format!("{}...", &feature.description[..100])
                                        } else {
                                            feature.description.clone()
                                        };
                                        let priority = feature.priority.clone();
                                        let pcls = priority_class(&priority);
                                        view! {
                                            <div class="roadmap-feature-card">
                                                <div class="roadmap-feature-header">
                                                    <span class="roadmap-feature-title">{title}</span>
                                                    <span class={format!("roadmap-status-badge {}", scls)}>
                                                        {status_label}
                                                    </span>
                                                </div>
                                                <div class="roadmap-feature-meta">
                                                    <span class={format!("roadmap-priority-badge {}", pcls)}>
                                                        {priority}
                                                    </span>
                                                    <button
                                                        class="btn btn-xs btn-outline roadmap-task-btn"
                                                        on:click={
                                                            let t = title_for_task.clone();
                                                            let d = desc_for_task.clone();
                                                            move |_| {
                                                                let t = t.clone();
                                                                let d = d.clone();
                                                                set_task_created_msg.set(None);
                                                                set_error_msg.set(None);
                                                                spawn_local(async move {
                                                                    let desc = if d.is_empty() { None } else { Some(d.as_str()) };
                                                                    match api::create_bead(&t, desc, Some("standard")).await {
                                                                        Ok(_bead) => {
                                                                            set_task_created_msg.set(Some(format!("Task created from '{}'", t)));
                                                                        }
                                                                        Err(e) => {
                                                                            set_error_msg.set(Some(format!("Failed to create task: {e}")));
                                                                        }
                                                                    }
                                                                });
                                                            }
                                                        }
                                                    >"Task"</button>
                                                </div>
                                                <div class="roadmap-feature-desc">{desc_snippet}</div>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            }
        })}

        {move || (!loading.get() && features.get().is_empty() && error_msg.get().is_none()).then(|| view! {
            <div class="dashboard-loading">"No roadmap items found. Click 'Generate Roadmap' to create one."</div>
        })}
    }
}
