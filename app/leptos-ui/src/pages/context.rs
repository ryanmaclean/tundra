use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;

#[component]
pub fn ContextPage() -> impl IntoView {
    let (entries, set_entries) = signal(Vec::<api::ApiMemoryEntry>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (search_query, set_search_query) = signal(String::new());
    let (show_add_form, set_show_add_form) = signal(false);
    let (new_category, set_new_category) = signal(String::new());
    let (new_content, set_new_content) = signal(String::new());

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::fetch_memory().await {
                Ok(data) => set_entries.set(data),
                Err(e) => set_error_msg.set(Some(format!("Failed to fetch memory: {e}"))),
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
                Err(e) => set_error_msg.set(Some(format!("Search failed: {e}"))),
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
                    // Refresh
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

    // Group entries by category
    let grouped_entries = move || {
        let all = entries.get();
        let mut groups: Vec<(String, Vec<api::ApiMemoryEntry>)> = Vec::new();
        for entry in all {
            let cat = if entry.category.is_empty() { "Uncategorized".to_string() } else { entry.category.clone() };
            if let Some(grp) = groups.iter_mut().find(|(c, _)| *c == cat) {
                grp.1.push(entry);
            } else {
                groups.push((cat, vec![entry]));
            }
        }
        groups
    };

    view! {
        <div class="page-header">
            <h2>"Context / Memory"</h2>
            <div class="page-header-actions">
                <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                    "\u{21BB} Refresh"
                </button>
                <button
                    class="action-btn action-forward"
                    on:click=move |_| set_show_add_form.set(!show_add_form.get())
                >
                    "+ Add Entry"
                </button>
            </div>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error">{msg}</div>
        })}

        // Search box
        <div class="filter-bar" style="margin-bottom: 16px;">
            <div class="filter-group filter-search-group" style="flex: 1;">
                <label class="filter-label">"Search Memory"</label>
                <input
                    type="text"
                    class="filter-search"
                    placeholder="Search memory entries..."
                    prop:value=move || search_query.get()
                    on:input=move |ev| set_search_query.set(event_target_value(&ev))
                    on:keydown=move |ev| {
                        if ev.key() == "Enter" {
                            do_search();
                        }
                    }
                />
            </div>
            <button class="action-btn action-start" style="margin-top: 20px;" on:click=move |_| do_search()>
                "Search"
            </button>
        </div>

        // Add entry form
        {move || show_add_form.get().then(|| view! {
            <div class="roadmap-add-form">
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

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">"Loading memory entries..."</div>
        })}

        // Grouped display
        {move || grouped_entries().into_iter().map(|(category, items)| {
            view! {
                <div class="section" style="margin-bottom: 16px;">
                    <h3>{category}</h3>
                    <div class="activity-feed">
                        {items.into_iter().map(|entry| {
                            view! {
                                <div class="activity-item" style="padding: 8px 12px;">
                                    <div style="display: flex; justify-content: space-between; align-items: start;">
                                        <div style="flex: 1;">{entry.content}</div>
                                        <span style="font-size: 0.75em; color: #8b949e; margin-left: 12px; white-space: nowrap;">
                                            {entry.created_at}
                                        </span>
                                    </div>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                </div>
            }
        }).collect::<Vec<_>>()}

        {move || (!loading.get() && entries.get().is_empty() && error_msg.get().is_none()).then(|| view! {
            <div class="dashboard-loading">"No memory entries found."</div>
        })}
    }
}
