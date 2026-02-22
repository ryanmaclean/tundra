use leptos::prelude::*;
use crate::state::use_app_state;
use crate::themed::{themed, Prompt};
use leptos::task::spawn_local;

use crate::api;
use crate::i18n::t;

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
                let path = s.general.workspace_root.unwrap_or_else(|| s.general.project_name.clone());
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
                        "Codebase" => cat.contains("codebase") || cat.contains("code") || cat.contains("architecture"),
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
        // Tab bar
        <div class="context-tabs">
            <button
                class=move || if active_tab.get() == 0 { "context-tab active" } else { "context-tab" }
                on:click=move |_| set_active_tab.set(0)
            >
                <span class="context-tab-icon">"\u{2699}"</span>
                {t("context-project-index")}
            </button>
            <button
                class=move || if active_tab.get() == 1 { "context-tab active" } else { "context-tab" }
                on:click=move |_| set_active_tab.set(1)
            >
                <span class="context-tab-icon">"\u{29BE}"</span>
                "Memories"
            </button>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error" style="margin: 16px;">{msg}</div>
        })}

        // ── Project Index Tab ──
        {move || (active_tab.get() == 0).then(|| view! {
            <div>
                <div class="page-header" style="border-bottom: none;">
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
                        <span class="graph-memory-icon">"\u{1F4E6}"</span>
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
                    <span class="status-badge status-badge-unavailable">
                        "\u{26D4} Not Available"
                    </span>
                </div>

                // Add entry button
                <div style="display: flex; justify-content: flex-end; padding: 12px 16px 0;">
                    <button
                        class="action-btn action-forward"
                        on:click=move |_| set_show_add_form.set(!show_add_form.get())
                    >
                        "+ Add Entry"
                    </button>
                </div>

                // Add entry form
                {move || show_add_form.get().then(|| view! {
                    <div class="roadmap-add-form" style="margin: 8px 16px;">
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
                <div style="display: flex; gap: 8px; padding: 0 16px 12px; align-items: center;">
                    <input
                        type="text"
                        class="filter-search"
                        style="flex: 1;"
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
                        class="refresh-btn"
                        style="height: 32px; padding: 0 12px;"
                        on:click=move |_| do_search()
                    >
                        "\u{1F50D}"
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
                            let icon = match chip {
                                "All" => "\u{1F4CB}",
                                "PR Reviews" => "\u{1F4DD}",
                                "Sessions" => "\u{1F4AC}",
                                "Codebase" => "\u{1F4E6}",
                                "Patterns" => "\u{2699}",
                                "Gotchas" => "\u{26A0}",
                                _ => "",
                            };
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
                                    <span class="memory-chip-icon">{icon}</span>
                                    {chip}
                                </button>
                            }
                        })
                        .collect::<Vec<_>>()
                    }
                </div>

                // Loading
                {move || loading.get().then(|| view! {
                    <div class="dashboard-loading" style="padding: 0 16px;">{move || themed(display_mode.get(), Prompt::Loading)}</div>
                })}

                // Empty state
                {move || {
                    let items = filtered_entries();
                    (!loading.get() && items.is_empty()).then(|| view! {
                        <div class="memory-empty">
                            <div class="memory-empty-icon">"\u{1F9E0}"</div>
                            <div class="memory-empty-text">
                                "No memories recorded yet. Memories are created during AI agent sessions and PR reviews."
                            </div>
                        </div>
                    })
                }}

                // Memory entries list
                {move || {
                    let items = filtered_entries();
                    (!items.is_empty()).then(|| view! {
                        <div class="memory-entries-list">
                            <div class="activity-feed">
                                {items.into_iter().map(|entry| {
                                    view! {
                                        <div class="activity-item memory-entry-item">
                                            <div class="memory-entry-row">
                                                <div class="memory-entry-content">
                                                    <div class="memory-entry-tags">
                                                        <span class="tag tag-default">{entry.category.clone()}</span>
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
    }
}
