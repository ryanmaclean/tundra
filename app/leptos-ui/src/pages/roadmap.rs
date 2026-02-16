use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;

#[component]
pub fn RoadmapPage() -> impl IntoView {
    let (features, set_features) = signal(Vec::<api::ApiRoadmapItem>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (expanded_id, set_expanded_id) = signal(Option::<String>::None);
    let (generating, set_generating) = signal(false);
    let (show_add_form, set_show_add_form) = signal(false);
    let (new_title, set_new_title) = signal(String::new());
    let (new_desc, set_new_desc) = signal(String::new());
    let (new_priority, set_new_priority) = signal("Medium".to_string());

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

    let on_add_feature = move |_| {
        let title = new_title.get();
        let desc = new_desc.get();
        let priority = new_priority.get();
        if title.trim().is_empty() {
            return;
        }
        // Add locally (POST endpoint may not exist yet)
        let feature = api::ApiRoadmapItem {
            id: format!("feat-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("000")),
            title,
            description: desc,
            status: "Planned".to_string(),
            priority,
        };
        set_features.update(|f| f.push(feature));
        set_new_title.set(String::new());
        set_new_desc.set(String::new());
        set_new_priority.set("Medium".to_string());
        set_show_add_form.set(false);
    };

    let status_class = |status: &str| -> &'static str {
        match status {
            "Planned" | "planned" => "roadmap-badge-planned",
            "InProgress" | "in_progress" | "In Progress" => "roadmap-badge-inprogress",
            "Done" | "done" | "completed" => "roadmap-badge-done",
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

    view! {
        <div class="page-header">
            <h2>"Roadmap"</h2>
            <div class="page-header-actions">
                <button
                    class="action-btn action-start"
                    on:click=on_generate
                    disabled=move || generating.get()
                >
                    {move || if generating.get() { "Generating..." } else { "Generate Roadmap" }}
                </button>
                <button
                    class="action-btn action-forward"
                    on:click=move |_| set_show_add_form.set(!show_add_form.get())
                >
                    "+ Add Feature"
                </button>
                <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                    "\u{21BB} Refresh"
                </button>
            </div>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">"Loading roadmap..."</div>
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
                    <button class="action-btn action-start" on:click=on_add_feature>
                        "Add Feature"
                    </button>
                </div>
            </div>
        })}

        // Feature grid
        <div class="roadmap-grid">
            {move || features.get().into_iter().map(|feature| {
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

        {move || (!loading.get() && features.get().is_empty() && error_msg.get().is_none()).then(|| view! {
            <div class="dashboard-loading">"No roadmap items found. Click 'Generate Roadmap' to create one."</div>
        })}
    }
}
