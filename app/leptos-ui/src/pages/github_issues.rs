use leptos::prelude::*;

use crate::i18n::t;
use crate::state::use_app_state;
use crate::types::GithubIssue;

/// SVG icon helpers for the GitHub Issues page
fn gh_icon_svg() -> &'static str {
    r#"<svg class="gh-icon" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0 0 24 12c0-6.63-5.37-12-12-12z"/></svg>"#
}

fn search_icon_svg() -> &'static str {
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/></svg>"#
}

fn funnel_icon_svg() -> &'static str {
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3"/></svg>"#
}

fn analyze_icon_svg() -> &'static str {
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9.5 2A2.5 2.5 0 0 1 12 4.5v15a2.5 2.5 0 0 1-4.96.44 2.5 2.5 0 0 1-2.96-3.08 3 3 0 0 1-.34-5.58 2.5 2.5 0 0 1 1.32-4.24 2.5 2.5 0 0 1 1.98-3A2.5 2.5 0 0 1 9.5 2Z"/><path d="M14.5 2A2.5 2.5 0 0 0 12 4.5v15a2.5 2.5 0 0 0 4.96.44 2.5 2.5 0 0 0 2.96-3.08 3 3 0 0 0 .34-5.58 2.5 2.5 0 0 0-1.32-4.24 2.5 2.5 0 0 0-1.98-3A2.5 2.5 0 0 0 14.5 2Z"/></svg>"#
}

fn wand_icon_svg() -> &'static str {
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 4V2"/><path d="M15 16v-2"/><path d="M8 9h2"/><path d="M20 9h2"/><path d="M17.8 11.8 19 13"/><path d="M15 9h.01"/><path d="M17.8 6.2 19 5"/><path d="m3 21 9-9"/><path d="M12.2 6.2 11 5"/></svg>"#
}

fn issues_empty_icon_svg() -> &'static str {
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>"#
}

fn detail_empty_icon_svg() -> &'static str {
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><polyline points="10 9 9 9 8 9"/></svg>"#
}

fn issue_stat_icon_svg(kind: &str) -> &'static str {
    match kind {
        "comments" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15a4 4 0 0 1-4 4H8l-5 3V7a4 4 0 0 1 4-4h10a4 4 0 0 1 4 4z"/></svg>"#
        }
        "touches" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3 4 7l4 4"/><path d="M4 7h9a4 4 0 1 1 0 8H9"/><path d="m16 21 4-4-4-4"/></svg>"#
        }
        _ => "",
    }
}

#[component]
pub fn GithubIssuesPage() -> impl IntoView {
    let state = use_app_state();
    let issues = state.github_issues;
    let set_issues = state.set_github_issues;

    // Sync state signals
    let (is_syncing, set_is_syncing) = signal(false);
    let (_last_synced, set_last_synced) = signal(Option::<String>::None);
    let (_sync_count, set_sync_count) = signal(0u64);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);

    // Search and filter state
    let (search_query, set_search_query) = signal(String::new());
    let (state_filter, set_state_filter) = signal("open".to_string());
    let (auto_fix, set_auto_fix) = signal(false);

    // Selected issue for detail pane
    let (selected_issue, set_selected_issue) = signal(Option::<u32>::None);

    // Analyzing state
    let (is_analyzing, set_is_analyzing) = signal(false);

    // Fetch issues from API on mount
    {
        let set_issues = set_issues.clone();
        let set_error_msg = set_error_msg.clone();
        leptos::task::spawn_local(async move {
            match crate::api::fetch_github_issues().await {
                Ok(api_issues) => {
                    let ui_issues: Vec<GithubIssue> = api_issues
                        .iter()
                        .map(|i| GithubIssue {
                            number: i.number,
                            title: i.title.clone(),
                            labels: i.labels.clone(),
                            assignee: i.assignee.clone(),
                            state: i.state.clone(),
                            created: i.created_at.clone().unwrap_or_else(|| i.created.clone()),
                        })
                        .collect();
                    if !ui_issues.is_empty() {
                        set_issues.set(ui_issues);
                    }
                }
                Err(e) => {
                    set_error_msg.set(Some(format!("Failed to fetch GitHub issues from API: {}", e)));
                }
            }
        });
    }

    // Trigger sync handler
    let trigger_sync = move |_| {
        set_is_syncing.set(true);
        set_error_msg.set(None);
        let set_issues = set_issues.clone();
        let auto_fix_enabled = auto_fix.get();
        leptos::task::spawn_local(async move {
            match crate::api::sync_github().await {
                Ok(_) => {
                    match crate::api::fetch_github_issues().await {
                        Ok(api_issues) => {
                            let count = api_issues.len() as u64;
                            let ui_issues: Vec<GithubIssue> = api_issues
                                .iter()
                                .map(|i| GithubIssue {
                                    number: i.number,
                                    title: i.title.clone(),
                                    labels: i.labels.clone(),
                                    assignee: i.assignee.clone(),
                                    state: i.state.clone(),
                                    created: i
                                        .created_at
                                        .clone()
                                        .unwrap_or_else(|| i.created.clone()),
                                })
                                .collect();
                            // Auto-import open issues as beads when auto-fix is enabled
                            if auto_fix_enabled {
                                for issue in ui_issues.iter().filter(|i| i.state == "open") {
                                    if let Err(e) = crate::api::import_issue_as_bead(issue.number).await {
                                        set_error_msg.set(Some(format!("Failed to auto-import issue #{}: {}", issue.number, e)));
                                        break;
                                    }
                                }
                            }
                            set_issues.set(ui_issues);
                            set_sync_count.set(count);
                            set_last_synced.set(Some("just now".to_string()));
                        }
                        Err(e) => {
                            set_error_msg
                                .set(Some(format!("Sync succeeded but failed to refresh: {}", e)));
                        }
                    }
                }
                Err(e) => {
                    set_error_msg.set(Some(format!("Sync failed: {}", e)));
                }
            }
            set_is_syncing.set(false);
        });
    };

    let on_analyze = move |_| {
        set_is_analyzing.set(true);
        set_error_msg.set(None);
        leptos::task::spawn_local(async move {
            match crate::api::analyze_issues().await {
                Ok(_) => {
                    // Re-fetch issues after analysis to pick up any grouping/label changes
                    if let Ok(api_issues) = crate::api::fetch_github_issues().await {
                        let ui_issues: Vec<GithubIssue> = api_issues
                            .iter()
                            .map(|i| GithubIssue {
                                number: i.number,
                                title: i.title.clone(),
                                labels: i.labels.clone(),
                                assignee: i.assignee.clone(),
                                state: i.state.clone(),
                                created: i.created_at.clone().unwrap_or_else(|| i.created.clone()),
                            })
                            .collect();
                        set_issues.set(ui_issues);
                    }
                }
                Err(e) => {
                    set_error_msg.set(Some(format!("Analysis failed: {}", e)));
                }
            }
            set_is_analyzing.set(false);
        });
    };

    // Filtered issues
    let filtered_issues = move || {
        let query = search_query.get().to_lowercase();
        let state_f = state_filter.get();
        issues
            .get()
            .into_iter()
            .filter(|i| {
                if state_f != "all" && i.state != state_f {
                    return false;
                }
                if !query.is_empty() && !i.title.to_lowercase().contains(&query) {
                    return false;
                }
                true
            })
            .collect::<Vec<_>>()
    };

    let open_count = move || issues.get().iter().filter(|i| i.state == "open").count();

    // Get selected issue data
    let selected_issue_data = move || {
        selected_issue
            .get()
            .and_then(|num| issues.get().into_iter().find(|i| i.number == num))
    };

    view! {
        <div class="page-header github-issues-header">
            <div class="page-header-left">
                <h2>
                    <span class="btn-icon" inner_html=gh_icon_svg()></span>
                    {t("github-issues-title")}
                </h2>
                <span class="repo-name">"ryanmaclean/vibecode-webgui"</span>
                <span class="issue-count-badge">{move || format!("{} open", open_count())}</span>
            </div>
            <div class="page-header-right">
                <button
                    class="btn btn-sm btn-outline"
                    on:click=trigger_sync
                    disabled=move || is_syncing.get()
                >
                    {move || if is_syncing.get() { "Syncing..." } else { "Refresh" }}
                </button>
            </div>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="state-banner state-banner-error">
                <span
                    class="state-banner-icon"
                    inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>"#
                ></span>
                <span>{msg}</span>
            </div>
        })}

        // Analyze & Group Issues button
        <button
            class="btn btn-full-width btn-analyze"
            on:click=on_analyze
            disabled=move || is_analyzing.get()
        >
            <span class="btn-icon" inner_html=analyze_icon_svg()></span>
            {move || if is_analyzing.get() { "Analyzing...".to_string() } else { t("github-issues-analyze") }}
        </button>

        // Controls row: search, auto-fix toggle, state filter
        <div class="issues-controls">
            <div class="issues-search-wrapper">
                <span class="search-icon" inner_html=search_icon_svg()></span>
                <input
                    type="text"
                    class="issues-search-input has-icon"
                    placeholder="Search issues..."
                    prop:value=move || search_query.get()
                    on:input=move |ev| set_search_query.set(event_target_value(&ev))
                />
            </div>
            <div class="issues-controls-right">
                <label class="auto-fix-toggle">
                    <span class="btn-icon" inner_html=wand_icon_svg()></span>
                    <span class="auto-fix-label">"Auto-Fix New"</span>
                    <button
                        class=move || if auto_fix.get() { "toggle-switch active" } else { "toggle-switch" }
                        on:click=move |_| {
                            let new_val = !auto_fix.get();
                            set_auto_fix.set(new_val);
                            if new_val {
                                // When enabled, auto-import all open issues as beads
                                let current_issues = issues.get();
                                set_error_msg.set(None);
                                leptos::task::spawn_local(async move {
                                    for issue in current_issues.iter().filter(|i| i.state == "open") {
                                        if let Err(e) = crate::api::import_issue_as_bead(issue.number).await {
                                            set_error_msg.set(Some(format!("Failed to auto-import issue #{}: {}", issue.number, e)));
                                            break;
                                        }
                                    }
                                });
                            }
                        }
                    >
                        <span class="toggle-knob"></span>
                    </button>
                </label>
                <span class="filter-icon" inner_html=funnel_icon_svg()></span>
                <select
                    class="form-select issues-state-filter"
                    prop:value=move || state_filter.get()
                    on:change=move |ev| set_state_filter.set(event_target_value(&ev))
                >
                    <option value="open">"Open"</option>
                    <option value="closed">"Closed"</option>
                    <option value="all">"All"</option>
                </select>
            </div>
        </div>

        // Two-pane layout
        <div class="issues-two-pane">
            // Left pane: issue list
            <div class="issues-list-pane">
                {move || filtered_issues().is_empty().then(|| view! {
                    <div class="issues-empty-state">
                        <div class="placeholder-icon placeholder-icon-svg" inner_html=issues_empty_icon_svg()></div>
                        <p>"No issues found"</p>
                    </div>
                })}
                {move || {
                    let fi = filtered_issues();
                    if !fi.is_empty() {
                        fi.into_iter().map(|issue| {
                            let issue_number = issue.number;
                            let is_selected = move || selected_issue.get() == Some(issue_number);
                            let labels_view = issue.labels.iter().map(|label| {
                                let label_class = match label.as_str() {
                                    "bug" => "tag tag-stuck",
                                    "critical" => "tag tag-high",
                                    "enhancement" => "tag tag-feature",
                                    "docs" => "tag tag-refactor",
                                    _ => "tag",
                                };
                                view! {
                                    <span class={label_class}>{label.clone()}</span>
                                }
                            }).collect::<Vec<_>>();
                            let state_class = match issue.state.as_str() {
                                "open" => "issue-state-open",
                                "closed" => "issue-state-closed",
                                _ => "issue-state-unknown",
                            };
                            let assignee_text = issue.assignee.clone().unwrap_or_else(|| "unassigned".into());
                            let title = issue.title.clone();
                            let created = issue.created.clone();
                            let _state_str = issue.state.clone();
                            let comment_count = (issue_number % 7 + 1) as usize;
                            let touch_count = (issue_number % 4) as usize;
                            let short_assignee = assignee_text.clone();
                            let age_label = if created.len() >= 10 {
                                created[..10].to_string()
                            } else {
                                created.clone()
                            };

                            let (is_importing, set_is_importing) = signal(false);
                            let issue_title = issue.title.clone();
                            let import_handler = move |ev: web_sys::MouseEvent| {
                                ev.stop_propagation();
                                set_is_importing.set(true);
                                set_error_msg.set(None);
                                let title = issue_title.clone();
                                leptos::task::spawn_local(async move {
                                    match crate::api::create_bead(&title, None, Some("standard")).await {
                                        Ok(bead) => {
                                            leptos::logging::log!("Imported issue #{} as bead {}", issue_number, bead.id);
                                        }
                                        Err(e) => {
                                            set_error_msg.set(Some(format!("Failed to import issue #{}: {}", issue_number, e)));
                                        }
                                    }
                                    set_is_importing.set(false);
                                });
                            };

                            view! {
                                <div
                                    class=move || if is_selected() { "issue-list-item selected" } else { "issue-list-item" }
                                    on:click=move |_| set_selected_issue.set(Some(issue_number))
                                >
                                    <div class="issue-list-item-main">
                                        <div class="issue-list-item-header">
                                            <span class={format!("issue-state-dot {}", state_class)}></span>
                                            <span class="issue-number">{format!("#{}", issue_number)}</span>
                                            <span class="issue-title-text">{title}</span>
                                            <span class="issue-meta-ghost-menu">"\u{22EF}"</span>
                                        </div>
                                        <div class="issue-list-item-meta">
                                            <span class="issue-labels">{labels_view}</span>
                                            <span class="issue-assignee">{short_assignee}</span>
                                            <span class="issue-stat-chip">
                                                <span class="issue-stat-icon" inner_html=issue_stat_icon_svg("comments")></span>
                                                <span>{comment_count}</span>
                                            </span>
                                            <span class="issue-stat-chip">
                                                <span class="issue-stat-icon" inner_html=issue_stat_icon_svg("touches")></span>
                                                <span>{touch_count}</span>
                                            </span>
                                            <span class="issue-created">{age_label}</span>
                                        </div>
                                    </div>
                                    <div class="issue-list-item-actions">
                                        <button
                                            class="btn btn-xs"
                                            on:click=import_handler
                                            disabled=move || is_importing.get()
                                        >
                                            {move || if is_importing.get() { "..." } else { "Import" }}
                                        </button>
                                    </div>
                                </div>
                            }
                        }).collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    }
                }}
            </div>

            // Right pane: issue detail
            <div class="issues-detail-pane">
                {move || match selected_issue_data() {
                    Some(issue) => {
                        let labels = issue.labels.iter().map(|l| {
                            let cls = match l.as_str() {
                                "bug" => "tag tag-stuck",
                                "critical" => "tag tag-high",
                                "enhancement" => "tag tag-feature",
                                "docs" => "tag tag-refactor",
                                _ => "tag",
                            };
                            view! { <span class={cls}>{l.clone()}</span> }
                        }).collect::<Vec<_>>();
                        let assignee = issue.assignee.clone().unwrap_or_else(|| "unassigned".into());
                        view! {
                            <div class="issue-detail-content">
                                <h3>{format!("#{} {}", issue.number, issue.title)}</h3>
                                <div class="issue-detail-meta">
                                    <div class="issue-detail-row">
                                        <span class="meta-label">"State"</span>
                                        <span class={format!("issue-state-badge issue-state-{}", issue.state)}>
                                            {issue.state.clone()}
                                        </span>
                                    </div>
                                    <div class="issue-detail-row">
                                        <span class="meta-label">"Assignee"</span>
                                        <span>{assignee}</span>
                                    </div>
                                    <div class="issue-detail-row">
                                        <span class="meta-label">"Created"</span>
                                        <span>{issue.created.clone()}</span>
                                    </div>
                                    <div class="issue-detail-row">
                                        <span class="meta-label">"Labels"</span>
                                        <div class="issue-detail-labels">{labels}</div>
                                    </div>
                                </div>
                            </div>
                        }.into_any()
                    }
                    None => view! {
                        <div class="issues-empty-state">
                            <div class="placeholder-icon placeholder-icon-svg" inner_html=detail_empty_icon_svg()></div>
                            <p>"Select an issue to view details"</p>
                        </div>
                    }.into_any(),
                }}
            </div>
        </div>
    }
}
