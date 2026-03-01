use crate::state::use_app_state;
use crate::themed::{themed, Prompt};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::components::spinner::Spinner;
use crate::i18n::t;

fn context_tab_icon_svg(kind: &str) -> &'static str {
    match kind {
        "project" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 7h18"/><path d="M6 3h12l2 4H4z"/><path d="M5 7l1 14h12l1-14"/></svg>"#
        }
        "memory" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 3a4 4 0 0 0-4 4v1a4 4 0 0 0-2 3.5A3.5 3.5 0 0 0 9.5 15H10v2a2 2 0 1 0 4 0v-2h.5a3.5 3.5 0 0 0 3.5-3.5A4 4 0 0 0 16 8V7a4 4 0 0 0-4-4z"/></svg>"#
        }
        "memory-status" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 2a7 7 0 0 0-7 7c0 2.1.9 3.8 2.4 5A3 3 0 0 1 8.5 16H15a3 3 0 0 1 1.1-2c1.5-1.2 2.4-3 2.4-5a7 7 0 0 0-7-7z"/><path d="M9 20h6"/><path d="M10 17h4"/></svg>"#
        }
        "search" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="8"/><path d="M21 21l-4.3-4.3"/></svg>"#
        }
        "empty-search" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><circle cx="11" cy="11" r="7"/><path d="M20 20l-4-4"/><path d="M9 11h4"/></svg>"#
        }
        "empty-filter" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M3 5h18"/><path d="M6 12h12"/><path d="M10 19h4"/></svg>"#
        }
        "empty-error" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><circle cx="12" cy="12" r="9"/><path d="M12 8v5"/><path d="M12 16h.01"/></svg>"#
        }
        "empty" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M12 2a7 7 0 0 0-7 7c0 2.1.9 3.8 2.4 5A3 3 0 0 1 8.5 16H15a3 3 0 0 1 1.1-2c1.5-1.2 2.4-3 2.4-5a7 7 0 0 0-7-7z"/><path d="M9 20h6"/><path d="M10 17h4"/></svg>"#
        }
        _ => "",
    }
}

fn memory_category_icon_svg(category: &str) -> &'static str {
    let c = category.to_lowercase();
    if c == "all" {
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 7h16"/><path d="M7 4h10l2 3H5z"/><path d="M6 7l1 13h10l1-13"/></svg>"#
    } else if c.contains("pr") || c.contains("review") {
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="6" cy="6" r="3"/><circle cx="18" cy="18" r="3"/><path d="M9 6h5a4 4 0 0 1 4 4v5"/><path d="m12 16 2 2 4-4"/></svg>"#
    } else if c.contains("session") {
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="4" width="18" height="14" rx="2"/><path d="M8 20h8"/><path d="M12 18v2"/></svg>"#
    } else if c.contains("code") || c.contains("arch") || c.contains("context") {
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="m8 6-4 6 4 6"/><path d="m16 6 4 6-4 6"/></svg>"#
    } else if c.contains("pattern") {
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 4h6v6H4z"/><path d="M14 4h6v6h-6z"/><path d="M4 14h6v6H4z"/><path d="M14 14h6v6h-6z"/></svg>"#
    } else if c.contains("gotcha") || c.contains("warning") || c.contains("risk") {
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 9v4"/><path d="M12 17h.01"/><path d="m10.3 3.6-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.7-3.4l-8-14a2 2 0 0 0-3.4 0z"/></svg>"#
    } else {
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9"/></svg>"#
    }
}

#[component]
pub fn ContextPage() -> impl IntoView {
    let app_state = use_app_state();
    let display_mode = app_state.display_mode;
    // Tab state: 0 = Project Index, 1 = Memories
    let (active_tab, set_active_tab) = signal(0u8);

    // Memory state
    let (entries, set_entries) = signal(Vec::<api::ApiMemoryEntry>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (search_query, set_search_query) = signal(String::new());
    let (last_query, set_last_query) = signal(String::new());
    let (show_add_form, set_show_add_form) = signal(false);
    let (new_category, set_new_category) = signal(String::new());
    let (new_content, set_new_content) = signal(String::new());
    let (active_filter, set_active_filter) = signal("All".to_string());

    // Settings for project path
    let (project_path, set_project_path) = signal(String::new());
    let (_settings_loaded, set_settings_loaded) = signal(false);

    // Load settings for workspace path
    spawn_local(async move {
        match api::fetch_settings().await {
            Ok(s) => {
                let path = s
                    .general
                    .workspace_root
                    .unwrap_or_else(|| s.general.project_name.clone());
                if path.is_empty() {
                    set_project_path.set("/Users/studio/rust-harness".to_string());
                } else {
                    set_project_path.set(path);
                }
            }
            Err(_) => {
                set_project_path.set("/Users/studio/rust-harness".to_string());
            }
        }
        set_settings_loaded.set(true);
    });

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        set_last_query.set(String::new());
        spawn_local(async move {
            match api::fetch_memory().await {
                Ok(data) => set_entries.set(data),
                Err(e) => {
                    if e.contains("404") || e.contains("Not Found") {
                        set_entries.set(Vec::new());
                    } else {
                        set_error_msg.set(Some(format!("Failed to fetch memory: {e}")));
                    }
                }
            }
            set_loading.set(false);
        });
    };

    do_refresh();

    let do_search = move || {
        let q = search_query.get();
        if q.trim().is_empty() {
            do_refresh();
            return;
        }
        set_last_query.set(q.clone());
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::search_memory(&q).await {
                Ok(data) => set_entries.set(data),
                Err(e) => {
                    if e.contains("404") || e.contains("Not Found") {
                        set_entries.set(Vec::new());
                    } else {
                        set_error_msg.set(Some(format!("Search failed: {e}")));
                    }
                }
            }
            set_loading.set(false);
        });
    };

    let on_add = move |_| {
        let cat = new_category.get();
        let content = new_content.get();
        if cat.trim().is_empty() || content.trim().is_empty() {
            return;
        }
        spawn_local(async move {
            match api::add_memory(&cat, &content).await {
                Ok(_) => {
                    set_new_category.set(String::new());
                    set_new_content.set(String::new());
                    set_show_add_form.set(false);
                    match api::fetch_memory().await {
                        Ok(data) => set_entries.set(data),
                        Err(_) => {}
                    }
                }
                Err(e) => {
                    set_error_msg.set(Some(format!("Failed to add entry: {e}")));
                }
            }
        });
    };

    // Filter entries by category chip
    let filtered_entries = move || {
        let all = entries.get();
        let filter = active_filter.get();
        if filter == "All" {
            all
        } else {
            all.into_iter()
                .filter(|e| {
                    let cat = e.category.to_lowercase();
                    match filter.as_str() {
                        "PR Reviews" => cat.contains("pr") || cat.contains("review"),
                        "Sessions" => cat.contains("session"),
                        "Codebase" => {
                            cat.contains("codebase")
                                || cat.contains("code")
                                || cat.contains("architecture")
                        }
                        "Patterns" => cat.contains("pattern"),
                        "Gotchas" => cat.contains("gotcha") || cat.contains("warning"),
                        _ => true,
                    }
                })
                .collect()
        }
    };

    let total_count = move || entries.get().len();
    let filtered_count = move || filtered_entries().len();

    view! {
        <div class="context-page">
        // Tab bar
        <div class="context-tabs">
            <button
                class=move || if active_tab.get() == 0 { "context-tab active" } else { "context-tab" }
                on:click=move |_| set_active_tab.set(0)
            >
                <span class="context-tab-icon context-tab-icon-svg" inner_html=context_tab_icon_svg("project")></span>
                {t("context-project-index")}
            </button>
            <button
                class=move || if active_tab.get() == 1 { "context-tab active" } else { "context-tab" }
                on:click=move |_| set_active_tab.set(1)
            >
                <span class="context-tab-icon context-tab-icon-svg" inner_html=context_tab_icon_svg("memory")></span>
                "Memories"
            </button>
        </div>

        {move || {
            if active_tab.get() == 0 {
                error_msg.get().map(|msg| view! {
                    <div class="dashboard-error context-error">{msg}</div>
                }).into_any()
            } else {
                view! { <></> }.into_any()
            }
        }}

        // ── Project Index Tab ──
        {move || (active_tab.get() == 0).then(|| view! {
            <div>
                <div class="page-header context-page-header">
                    <div>
                        <h2>"Project Structure"</h2>
                        <span class="mcp-header-subtitle">"AI-discovered knowledge about your codebase"</span>
                    </div>
                    <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                        "\u{21BB} Refresh"
                    </button>
                </div>

                <div class="project-overview-card">
                    <div class="project-overview-title">"Overview"</div>
                    <div class="project-badge">"Single"</div>
                    <div>
                        <span class="project-path">{move || project_path.get()}</span>
                    </div>
                </div>
            </div>
        })}

        // ── Memories Tab ──
        {move || (active_tab.get() == 1).then(|| view! {
            <div>
                // Graph Memory Status banner
                <div class="graph-memory-banner">
                    <div class="graph-memory-left">
                        <span class="graph-memory-icon graph-memory-icon-svg" inner_html=context_tab_icon_svg("memory-status")></span>
                        <div>
                            <div class="graph-memory-title">"Graph Memory Status"</div>
                            <div class="graph-memory-detail">
                                "OPENAI_API_KEY not set (required for OpenAI embeddings)"
                            </div>
                            <div class="graph-memory-hint">
                                "To enable graph memory, set "
                                <code>"GRAPHITI_ENABLED=true"</code>
                                " in project settings."
                            </div>
                        </div>
                    </div>
                    <span class="status-badge status-badge-unavailable">"Not Available"</span>
                </div>

                // Add entry button
                <div class="context-add-row">
                    <button
                        class="action-btn action-forward"
                        on:click=move |_| set_show_add_form.set(!show_add_form.get())
                    >
                        "+ Add Entry"
                    </button>
                </div>

                // Add entry form
                {move || show_add_form.get().then(|| view! {
                    <div class="roadmap-add-form context-add-form">
                        <h3>"Add New Memory Entry"</h3>
                        <div class="roadmap-form-fields">
                            <input
                                type="text"
                                class="form-input"
                                placeholder="Category (e.g., architecture, decisions, context)..."
                                prop:value=move || new_category.get()
                                on:input=move |ev| set_new_category.set(event_target_value(&ev))
                            />
                            <textarea
                                class="form-textarea"
                                placeholder="Content..."
                                prop:value=move || new_content.get()
                                on:input=move |ev| set_new_content.set(event_target_value(&ev))
                            ></textarea>
                            <button class="action-btn action-start" on:click=on_add>
                                "Add Entry"
                            </button>
                        </div>
                    </div>
                })}

                // Search memories section
                <div class="memory-section-header">
                    <span class="memory-section-title">"SEARCH MEMORIES"</span>
                </div>
                <div class="memory-search-row">
                    <input
                        type="text"
                        class="filter-search memory-search-input"
                        placeholder="Search for patterns, insights, gotchas..."
                        prop:value=move || search_query.get()
                        on:input=move |ev| set_search_query.set(event_target_value(&ev))
                        on:keydown=move |ev| {
                            if ev.key() == "Enter" {
                                do_search();
                            }
                        }
                    />
                    <button
                        class="refresh-btn memory-search-btn"
                        on:click=move |_| do_search()
                    >
                        <span inner_html=context_tab_icon_svg("search")></span>
                    </button>
                </div>

                // Memory Browser section
                <div class="memory-section-header">
                    <span class="memory-section-title">{t("context-memory-browser")}</span>
                    <span class="memory-count">
                        {move || format!("{} of {} memories", filtered_count(), total_count())}
                    </span>
                </div>

                // Category chips
                <div class="memory-chips">
                    {["All", "PR Reviews", "Sessions", "Codebase", "Patterns", "Gotchas"]
                        .into_iter()
                        .map(|chip| {
                            let chip_str = chip.to_string();
                            let chip_str2 = chip.to_string();
                            let icon_svg = memory_category_icon_svg(chip);
                            view! {
                                <button
                                    class=move || {
                                        if active_filter.get() == chip_str {
                                            "memory-chip active"
                                        } else {
                                            "memory-chip"
                                        }
                                    }
                                    on:click=move |_| set_active_filter.set(chip_str2.clone())
                                >
                                    <span class="memory-chip-icon memory-chip-icon-svg" inner_html=icon_svg></span>
                                    {chip}
                                </button>
                            }
                        })
                        .collect::<Vec<_>>()
                    }
                </div>

                // Loading
                {move || loading.get().then(|| view! {
                    <div class="memory-skeleton-list">
                        <div class="memory-skeleton-row">
                            <div class="skeleton skeleton-badge"></div>
                            <div class="skeleton skeleton-title"></div>
                            <div class="skeleton skeleton-short"></div>
                        </div>
                        <div class="memory-skeleton-row">
                            <div class="skeleton skeleton-badge"></div>
                            <div class="skeleton skeleton-title"></div>
                            <div class="skeleton skeleton-short"></div>
                        </div>
                        <div class="memory-skeleton-row">
                            <div class="skeleton skeleton-badge"></div>
                            <div class="skeleton skeleton-title"></div>
                            <div class="skeleton skeleton-short"></div>
                        </div>
                        <div class="dashboard-loading context-loading"><Spinner size="md" label="Loading memories..."/></div>
                    </div>
                })}

                // Error state with retry action
                {move || {
                    let err = error_msg.get();
                    (!loading.get() && err.is_some()).then(|| {
                        let msg = err.unwrap_or_default();
                        view! {
                            <div class="memory-empty memory-empty-error">
                                <div class="memory-empty-icon memory-empty-icon-svg" inner_html=context_tab_icon_svg("empty-error")></div>
                                <div class="memory-empty-text">{msg}</div>
                                <button class="refresh-btn memory-empty-action" on:click=move |_| do_refresh()>
                                    "\u{21BB} Retry"
                                </button>
                            </div>
                        }
                    })
                }}

                // Empty state
                {move || {
                    let items = filtered_entries();
                    let filter = active_filter.get();
                    let q = last_query.get();
                    (!loading.get() && error_msg.get().is_none() && items.is_empty()).then(|| {
                        let (icon, message) = if !q.trim().is_empty() {
                            ("empty-search", format!("No memories matched \"{}\". Try a broader search term.", q))
                        } else if filter != "All" {
                            ("empty-filter", format!("No memories available for {}. Try a different filter.", filter))
                        } else {
                            ("empty", "No memories recorded yet. Memories are created during AI agent sessions and PR reviews.".to_string())
                        };
                        view! {
                            <div class="memory-empty">
                                <div class="memory-empty-icon memory-empty-icon-svg" inner_html=context_tab_icon_svg(icon)></div>
                                <div class="memory-empty-text">{message}</div>
                            </div>
                        }
                    })
                }}

                // Memory entries list
                {move || {
                    let items = filtered_entries();
                    (!items.is_empty()).then(|| view! {
                        <div class="memory-entries-list">
                            <div class="activity-feed">
                                {items.into_iter().map(|entry| {
                                    let icon_svg = memory_category_icon_svg(&entry.category);
                                    view! {
                                        <div class="activity-item memory-entry-item memory-entry-animate">
                                            <div class="memory-entry-row">
                                                <div class="memory-entry-content">
                                                    <div class="memory-entry-tags">
                                                        <span class="tag tag-default memory-category-tag">
                                                            <span class="memory-category-tag-icon memory-chip-icon-svg" inner_html=icon_svg></span>
                                                            {entry.category.clone()}
                                                        </span>
                                                    </div>
                                                    <div>{entry.content}</div>
                                                </div>
                                                <span class="memory-entry-date">
                                                    {entry.created_at}
                                                </span>
                                            </div>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    })
                }}
            </div>
        })}
        </div>
    }
}
