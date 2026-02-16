use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;

#[component]
pub fn IdeationPage() -> impl IntoView {
    let (ideas, set_ideas) = signal(Vec::<api::ApiIdea>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (filter_category, set_filter_category) = signal("All".to_string());
    let (expanded_id, set_expanded_id) = signal(Option::<String>::None);
    let (generating, set_generating) = signal(false);

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::fetch_ideas().await {
                Ok(data) => set_ideas.set(data),
                Err(e) => set_error_msg.set(Some(format!("Failed to fetch ideas: {e}"))),
            }
            set_loading.set(false);
        });
    };

    do_refresh();

    let on_generate = move |_| {
        set_generating.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::generate_ideas().await {
                Ok(data) => set_ideas.set(data),
                Err(e) => set_error_msg.set(Some(format!("Failed to generate ideas: {e}"))),
            }
            set_generating.set(false);
        });
    };

    let filtered_ideas = move || {
        let cat = filter_category.get();
        let all_ideas = ideas.get();
        if cat == "All" {
            all_ideas
        } else {
            all_ideas.into_iter().filter(|i| i.category == cat).collect()
        }
    };

    let impact_class = |level: &str| -> &'static str {
        match level {
            "High" | "high" => "idea-level-high",
            "Medium" | "medium" => "idea-level-medium",
            "Low" | "low" => "idea-level-low",
            _ => "idea-level-medium",
        }
    };

    let cat_class = |cat: &str| -> &'static str {
        match cat {
            "CodeImprovement" | "code" => "idea-cat-code",
            "Quality" | "quality" => "idea-cat-quality",
            "Documentation" | "docs" => "idea-cat-docs",
            "Performance" | "performance" => "idea-cat-perf",
            "Security" | "security" => "idea-cat-security",
            "UiUx" | "ui" => "idea-cat-uiux",
            _ => "idea-cat-all",
        }
    };

    view! {
        <div class="page-header">
            <h2>"Ideation"</h2>
            <div class="page-header-actions">
                <button
                    class="action-btn action-start"
                    on:click=on_generate
                    disabled=move || generating.get()
                >
                    {move || if generating.get() { "Generating..." } else { "Generate Ideas" }}
                </button>
                <select
                    class="filter-select"
                    prop:value=move || filter_category.get()
                    on:change=move |ev| set_filter_category.set(event_target_value(&ev))
                >
                    <option value="All">"All Categories"</option>
                    <option value="CodeImprovement">"Code Improvement"</option>
                    <option value="Quality">"Quality"</option>
                    <option value="Documentation">"Documentation"</option>
                    <option value="Performance">"Performance"</option>
                    <option value="Security">"Security"</option>
                    <option value="UiUx">"UI/UX"</option>
                </select>
                <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                    "\u{21BB} Refresh"
                </button>
            </div>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">"Loading ideas..."</div>
        })}

        <div class="ideation-grid">
            {move || filtered_ideas().into_iter().map(|idea| {
                let iid = idea.id.clone();
                let iid_click = idea.id.clone();
                let is_expanded = move || expanded_id.get().as_deref() == Some(&iid);
                let title = idea.title.clone();
                let desc_snippet = if idea.description.len() > 120 {
                    format!("{}...", &idea.description[..120])
                } else {
                    idea.description.clone()
                };
                let full_desc = idea.description.clone();
                let cat_label = idea.category.clone();
                let ccls = cat_class(&cat_label);
                let impact = idea.impact.clone();
                let effort = idea.effort.clone();
                let icls = impact_class(&impact);
                let ecls = impact_class(&effort);

                view! {
                    <div
                        class="ideation-card"
                        on:click=move |_| {
                            if expanded_id.get().as_deref() == Some(&iid_click) {
                                set_expanded_id.set(None);
                            } else {
                                set_expanded_id.set(Some(iid_click.clone()));
                            }
                        }
                    >
                        <div class="ideation-card-header">
                            <span class="ideation-card-title">{title}</span>
                            <span class={format!("ideation-cat-badge {}", ccls)}>
                                {cat_label}
                            </span>
                        </div>
                        <div class="ideation-card-levels">
                            <span class={format!("ideation-level-badge {}", icls)}>
                                {format!("Impact: {}", impact)}
                            </span>
                            <span class={format!("ideation-level-badge {}", ecls)}>
                                {format!("Effort: {}", effort)}
                            </span>
                        </div>
                        <div class="ideation-card-desc">
                            {move || if is_expanded() { full_desc.clone() } else { desc_snippet.clone() }}
                        </div>
                        <div class="ideation-card-actions">
                            <button
                                class="action-btn action-start"
                                on:click=move |ev| {
                                    ev.stop_propagation();
                                    web_sys::console::log_1(&"Convert to Task clicked".into());
                                }
                            >
                                "Convert to Task"
                            </button>
                        </div>
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>

        {move || (!loading.get() && ideas.get().is_empty() && error_msg.get().is_none()).then(|| view! {
            <div class="dashboard-loading">"No ideas found. Click 'Generate Ideas' to create some."</div>
        })}
    }
}
