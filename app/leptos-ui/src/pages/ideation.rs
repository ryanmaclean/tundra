use leptos::prelude::*;
use crate::state::use_app_state;
use crate::themed::{themed, Prompt};
use leptos::task::spawn_local;

use crate::api;
use crate::i18n::t;

#[component]
pub fn IdeationPage() -> impl IntoView {
    let app_state = use_app_state();
    let display_mode = app_state.display_mode;
    let (ideas, set_ideas) = signal(Vec::<api::ApiIdea>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (filter_category, set_filter_category) = signal("All".to_string());
    let (expanded_id, set_expanded_id) = signal(Option::<String>::None);
    let (generating, set_generating) = signal(false);
    let (convert_msg, set_convert_msg) = signal(Option::<String>::None);
    let (sort_by, set_sort_by) = signal("Impact".to_string());
    let (sort_asc, set_sort_asc) = signal(false);

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

    let categories: Vec<(&str, &str)> = vec![
        ("All", "All"),
        ("CodeImprovement", "Code Improvement"),
        ("Quality", "Quality"),
        ("Documentation", "Documentation"),
        ("Performance", "Performance"),
        ("Security", "Security"),
        ("UiUx", "UI/UX"),
    ];

    view! {
        <div class="page-header">
            <div class="page-header-title-row">
                <h2>{t("ideation-title")}</h2>
                <span class="ideation-count-badge">
                    {move || format!("{} ideas", ideas.get().len())}
                </span>
            </div>
            <p class="ideation-subtitle">"Generate ideas for your project"</p>
            <div class="page-header-actions">
                <button
                    class="action-btn action-start"
                    on:click=on_generate
                    disabled=move || generating.get()
                >
                    {move || if generating.get() { "Generating...".to_string() } else { t("ideation-generate") }}
                </button>
                <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                    "\u{21BB} Refresh"
                </button>
            </div>
        </div>

        <div class="ideation-filter-pills">
            {categories.into_iter().map(|(value, label)| {
                let value_str = value.to_string();
                let value_click = value.to_string();
                let label_str = label.to_string();
                let pill_class = value.to_lowercase().replace(" ", "");
                view! {
                    <button
                        class=(move || {
                            let base = format!("ideation-filter-pill pill-{}", pill_class);
                            if filter_category.get() == value_str {
                                format!("{} active", base)
                            } else {
                                base
                            }
                        })
                        on:click=move |_| set_filter_category.set(value_click.clone())
                    >
                        {label_str}
                    </button>
                }
            }).collect::<Vec<_>>()}
        </div>

        <div class="ideation-sort-controls">
            <span class="ideation-sort-label">"Sort:"</span>
            <select
                class="ideation-sort-select"
                prop:value=move || sort_by.get()
                on:change=move |ev| set_sort_by.set(event_target_value(&ev))
            >
                <option value="Impact">"Impact"</option>
                <option value="Effort">"Effort"</option>
                <option value="Category">"Category"</option>
            </select>
            <button
                class="ideation-sort-dir-btn"
                on:click=move |_| set_sort_asc.set(!sort_asc.get())
            >
                {move || if sort_asc.get() { "\u{2191}" } else { "\u{2193}" }}
            </button>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error">{msg}</div>
        })}

        {move || convert_msg.get().map(|msg| view! {
            <div class="changelog-success">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">{move || themed(display_mode.get(), Prompt::Loading)}</div>
        })}

        <div class="ideation-grid">
            {move || filtered_ideas().into_iter().map(|idea| {
                let iid = idea.id.clone();
                let iid_click = idea.id.clone();
                let is_expanded = move || expanded_id.get().as_deref() == Some(&iid);
                let title = idea.title.clone();
                let convert_title = idea.title.clone();
                let convert_desc = idea.description.clone();
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
                                on:click={
                                    let convert_title = convert_title.clone();
                                    let convert_desc = convert_desc.clone();
                                    move |ev: web_sys::MouseEvent| {
                                        ev.stop_propagation();
                                        let t = convert_title.clone();
                                        let d = convert_desc.clone();
                                        set_convert_msg.set(None);
                                        spawn_local(async move {
                                            match api::create_bead(&t, Some(&d), Some("standard")).await {
                                                Ok(bead) => set_convert_msg.set(Some(
                                                    format!("Created task '{}' (id: {})", bead.title, bead.id)
                                                )),
                                                Err(e) => set_convert_msg.set(Some(
                                                    format!("Failed to create task: {e}")
                                                )),
                                            }
                                        });
                                    }
                                }
                            >
                                {t("ideation-convert-task")}
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
