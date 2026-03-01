use crate::components::Spinner;
use crate::state::use_app_state;
use crate::themed::{themed, Prompt};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::i18n::t;

fn changelog_source_icon_svg(kind: &str) -> &'static str {
    match kind {
        "completed_tasks" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/><path d="m9 15 2 2 4-4"/></svg>"#
        }
        "git_history" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="6" cy="6" r="2"/><circle cx="6" cy="18" r="2"/><circle cx="18" cy="12" r="2"/><path d="M8 7.5 16 11"/><path d="M8 16.5 16 13"/></svg>"#
        }
        "branch_comparison" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 3v12"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/></svg>"#
        }
        _ => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9"/></svg>"#
        }
    }
}

#[component]
pub fn ChangelogPage() -> impl IntoView {
    let app_state = use_app_state();
    let display_mode = app_state.display_mode;
    let (entries, set_entries) = signal(Vec::<api::ApiChangelogEntry>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (offline_demo, set_offline_demo) = signal(false);
    let (success_msg, set_success_msg) = signal(Option::<String>::None);

    // 3-step generator flow: 0=hidden, 1=Select, 2=Generate, 3=Release
    let (gen_step, set_gen_step) = signal(0u8);

    // Step 1: Source selection
    let (changelog_source, set_changelog_source) = signal("completed_tasks".to_string());
    let (selected_tasks, set_selected_tasks) = signal(Vec::<String>::new());

    // Step 2: Generation
    let (gen_version, set_gen_version) = signal(default_version());
    let (gen_commits, set_gen_commits) = signal(String::new());
    let (generating, set_generating) = signal(false);
    let (generated_result, set_generated_result) = signal(Option::<api::ApiChangelogEntry>::None);

    // Step 3: Publish to GitHub
    let (publishing, set_publishing) = signal(false);

    // Expanded entry tracking
    let (expanded_version, set_expanded_version) = signal(Option::<String>::None);

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::fetch_changelog().await {
                Ok(data) => {
                    set_offline_demo.set(false);
                    let is_empty = data.is_empty();
                    set_entries.set(data);
                    if is_empty && gen_step.get_untracked() == 0 {
                        // Match the reference flow: show stepper immediately when no changelog exists.
                        set_gen_step.set(1);
                    }
                }
                Err(e) => {
                    if api::is_connection_error(&e) {
                        set_offline_demo.set(true);
                        set_entries.set(Vec::new());
                        set_error_msg.set(None);
                        set_gen_step.set(1);
                    } else {
                        set_error_msg.set(Some(format!("Failed to fetch changelog: {e}")));
                    }
                }
            }
            set_loading.set(false);
        });
    };

    do_refresh();

    let on_generate = move |_| {
        let version = gen_version.get();
        let commits = gen_commits.get();
        if commits.trim().is_empty() {
            set_error_msg.set(Some(
                "Please enter commits to generate changelog from.".to_string(),
            ));
            return;
        }
        set_generating.set(true);
        set_error_msg.set(None);
        set_success_msg.set(None);
        set_generated_result.set(None);
        spawn_local(async move {
            match api::generate_changelog(&commits, &version).await {
                Ok(entry) => {
                    set_success_msg.set(Some(format!(
                        "Changelog generated for version {}",
                        entry.version
                    )));
                    set_generated_result.set(Some(entry.clone()));
                    set_entries.update(|e| e.insert(0, entry));
                    set_gen_step.set(3); // Move to release step
                }
                Err(e) => set_error_msg.set(Some(format!("Failed to generate changelog: {e}"))),
            }
            set_generating.set(false);
        });
    };

    // Completed tasks loaded from API
    let (completed_tasks, set_completed_tasks) =
        signal(Vec::<(String, String, String, bool)>::new());
    let (tasks_loading, set_tasks_loading) = signal(false);

    // Load completed tasks from API on mount
    {
        set_tasks_loading.set(true);
        spawn_local(async move {
            match api::fetch_beads().await {
                Ok(beads) => {
                    set_offline_demo.set(false);
                    let done: Vec<(String, String, String, bool)> = beads
                        .into_iter()
                        .filter(|b| {
                            b.status == "done" || b.status == "completed" || b.lane == "done"
                        })
                        .map(|b| {
                            let desc = b.description.clone().unwrap_or_default();
                            let has_specs = !desc.is_empty();
                            (b.id, b.title, b.status.clone(), has_specs)
                        })
                        .collect();
                    set_completed_tasks.set(done);
                }
                Err(e) => {
                    if api::is_connection_error(&e) {
                        set_offline_demo.set(true);
                        set_completed_tasks.set(vec![
                            (
                                "task-1".to_string(),
                                "use local Mac Mini M4 24GB for testing virtual buddy VMs that can run public releases".to_string(),
                                "2026-02-14".to_string(),
                                false,
                            ),
                            (
                                "task-2".to_string(),
                                "Enterprise Audit Trail".to_string(),
                                "2026-02-14".to_string(),
                                true,
                            ),
                        ]);
                    } else {
                        set_error_msg.set(Some(format!("Failed to load completed tasks: {e}")));
                    }
                }
            }
            set_tasks_loading.set(false);
        });
    }

    let category_class = |cat: &str| -> &'static str {
        match cat.to_lowercase().as_str() {
            "added" => "changelog-cat-added",
            "changed" => "changelog-cat-changed",
            "fixed" => "changelog-cat-fixed",
            "removed" => "changelog-cat-removed",
            "deprecated" => "changelog-cat-deprecated",
            "security" => "changelog-cat-security",
            _ => "changelog-cat-other",
        }
    };

    let category_icon = |cat: &str| -> &'static str {
        match cat.to_lowercase().as_str() {
            "added" => "+",
            "changed" => "~",
            "fixed" => "!",
            "removed" => "-",
            "deprecated" => "D",
            "security" => "S",
            _ => "*",
        }
    };

    view! {
        <div class="page-header">
            <h2>{t("changelog-title")}</h2>
            <div class="page-header-actions changelog-header-actions">
                {move || (gen_step.get() == 0).then(|| view! {
                    <button
                        class="action-btn action-start"
                        on:click=move |_| set_gen_step.set(1)
                    >
                        {t("changelog-generate")}
                    </button>
                })}
                {move || (gen_step.get() > 0).then(|| view! {
                    <button
                        class="action-btn action-recover"
                        on:click=move |_| {
                            if entries.get_untracked().is_empty() { set_gen_step.set(1); } else { set_gen_step.set(0); }
                            set_generated_result.set(None);
                        }
                    >
                        "Cancel"
                    </button>
                })}
                <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                    "Refresh"
                </button>
            </div>
        </div>

        // Step indicator
        {move || (gen_step.get() > 0).then(|| {
            let step = gen_step.get();
            view! {
                <div class="changelog-subtitle">
                    {format!("Step {}: {}", step, match step {
                        1 => "Select completed tasks to include",
                        2 => "Generate",
                        3 => "Release",
                        _ => "",
                    })}
                </div>
                <div class="changelog-steps">
                    <div class=(move || if step >= 1 { "changelog-step active" } else { "changelog-step" })>
                        <span class="changelog-step-number">"1"</span>
                        <span class="changelog-step-label">"Select"</span>
                    </div>
                    <div class="changelog-step-connector"></div>
                    <div class=(move || if step >= 2 { "changelog-step active" } else { "changelog-step" })>
                        <span class="changelog-step-number">"2"</span>
                        <span class="changelog-step-label">"Generate"</span>
                    </div>
                    <div class="changelog-step-connector"></div>
                    <div class=(move || if step >= 3 { "changelog-step active" } else { "changelog-step" })>
                        <span class="changelog-step-number">"3"</span>
                        <span class="changelog-step-label">"Release"</span>
                    </div>
                </div>
            }
        })}

        {move || error_msg.get().map(|msg| view! {
            <div class="state-banner state-banner-error">
                <span
                    class="state-banner-icon"
                    inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>"#
                ></span>
                <span>{msg}</span>
            </div>
        })}

        {move || offline_demo.get().then(|| view! {
            <div class="state-banner state-banner-info">
                <span
                    class="state-banner-icon"
                    inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M12 8v4"/><path d="M12 16h.01"/></svg>"#
                ></span>
                <span>"Offline demo mode: completed tasks and generation flow are using local fallback data."</span>
            </div>
        })}

        {move || success_msg.get().map(|msg| view! {
            <div class="changelog-success">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading"><Spinner size="md" label="Loading changelog..."/></div>
        })}

        // Step 1: Select source and tasks
        {move || (gen_step.get() == 1).then(|| {
            view! {
                <div class="changelog-step-content">
                    // Changelog Source cards
                    <h3>"Changelog Source"</h3>
                    <div class="changelog-source-cards">
                        <div
                            class=move || if changelog_source.get() == "completed_tasks" { "changelog-source-card selected" } else { "changelog-source-card" }
                            on:click=move |_| set_changelog_source.set("completed_tasks".to_string())
                        >
                            <div class="changelog-source-icon" inner_html=changelog_source_icon_svg("completed_tasks")></div>
                            <div class="changelog-source-info">
                                <strong>"Completed Tasks"</strong>
                                <span class="changelog-source-badge">{move || format!("{}", completed_tasks.get().len())}</span>
                            </div>
                            <p class="changelog-source-desc">"Generate from completed spec tasks"</p>
                        </div>
                        <div
                            class=move || if changelog_source.get() == "git_history" { "changelog-source-card selected" } else { "changelog-source-card" }
                            on:click=move |_| set_changelog_source.set("git_history".to_string())
                        >
                            <div class="changelog-source-icon" inner_html=changelog_source_icon_svg("git_history")></div>
                            <div class="changelog-source-info">
                                <strong>"Git History"</strong>
                            </div>
                            <p class="changelog-source-desc">"Generate from recent commits or tag range"</p>
                        </div>
                        <div
                            class=move || if changelog_source.get() == "branch_comparison" { "changelog-source-card selected" } else { "changelog-source-card" }
                            on:click=move |_| set_changelog_source.set("branch_comparison".to_string())
                        >
                            <div class="changelog-source-icon" inner_html=changelog_source_icon_svg("branch_comparison")></div>
                            <div class="changelog-source-info">
                                <strong>"Branch Comparison"</strong>
                            </div>
                            <p class="changelog-source-desc">"Generate from commits between two branches"</p>
                        </div>
                    </div>

                    // Task selection (when completed_tasks source)
                    {move || (changelog_source.get() == "completed_tasks").then(|| {
                        let tasks = completed_tasks.get();
                        let sel_count = selected_tasks.get().len();
                        let total = tasks.len();
                        view! {
                            <div class="changelog-task-selection">
                                <div class="changelog-task-selection-header">
                                    <span>{format!("{} of {} tasks selected", sel_count, total)}</span>
                                    <div class="changelog-task-selection-actions">
                                        <button class="btn btn-xs btn-outline" on:click=move |_| {
                                            let all_ids: Vec<String> = completed_tasks.get().iter().map(|(id, _, _, _)| id.clone()).collect();
                                            set_selected_tasks.set(all_ids);
                                        }>"Select All"</button>
                                        <button class="btn btn-xs btn-outline" on:click=move |_| set_selected_tasks.set(vec![])>"Clear"</button>
                                    </div>
                                </div>
                                {move || tasks_loading.get().then(|| view! {
                                    <div class="dashboard-loading"><Spinner size="md" label="Loading tasks..."/></div>
                                })}
                                <div class="changelog-task-list">
                                    {tasks.iter().map(|(id, title, date, has_specs)| {
                                        let id = id.clone();
                                        let title = title.clone();
                                        let date = date.clone();
                                        let has_specs = *has_specs;
                                        let id_check = id.clone();
                                        let id_toggle = id.clone();
                                        view! {
                                            <div
                                                class=move || if selected_tasks.get().contains(&id_check) { "changelog-task-item selected" } else { "changelog-task-item" }
                                                on:click=move |_| {
                                                    set_selected_tasks.update(|sel| {
                                                        if sel.contains(&id_toggle) {
                                                            sel.retain(|s| s != &id_toggle);
                                                        } else {
                                                            sel.push(id_toggle.clone());
                                                        }
                                                    });
                                                }
                                            >
                                                <span class="changelog-task-check">
                                                    {move || if selected_tasks.get().contains(&id) { "[x]" } else { "[ ]" }}
                                                </span>
                                                <div class="changelog-task-info">
                                                    <span class="changelog-task-title">{title}</span>
                                                    <span class="changelog-task-date">{date}</span>
                                                </div>
                                                {has_specs.then(|| view! {
                                                    <span class="changelog-task-badge">"Has Specs"</span>
                                                })}
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>
                        }
                    })}

                    // Git history input (when git_history source)
                    {move || (changelog_source.get() == "git_history" || changelog_source.get() == "branch_comparison").then(|| view! {
                        <div class="changelog-git-input">
                            <label class="form-label">"Commits / Changes"</label>
                            <textarea
                                class="form-textarea"
                                rows="6"
                                placeholder="Paste commit messages or describe changes..."
                                prop:value=move || gen_commits.get()
                                on:input=move |ev| set_gen_commits.set(event_target_value(&ev))
                            ></textarea>
                        </div>
                    })}

                    // Continue button
                    <div class="changelog-step-actions">
                        <button
                            class="btn btn-primary btn-lg"
                            on:click=move |_| {
                                // When source is completed_tasks, populate gen_commits from selected tasks
                                if changelog_source.get() == "completed_tasks" {
                                    let sel = selected_tasks.get();
                                    let tasks = completed_tasks.get();
                                    let summary: String = tasks
                                        .iter()
                                        .filter(|(id, _, _, _)| sel.contains(id))
                                        .map(|(_, title, status, _)| format!("- {title} ({status})"))
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    set_gen_commits.set(summary);
                                }
                                set_gen_step.set(2);
                            }
                        >
                            "Continue \u{2192}"
                        </button>
                    </div>
                </div>
            }
        })}

        // Step 2: Generate
        {move || (gen_step.get() == 2).then(|| view! {
            <div class="changelog-step-content">
                <div class="roadmap-form-fields">
                    <label class="form-label">"Version"</label>
                    <input
                        type="text"
                        class="form-input"
                        placeholder="e.g. 2026.02.16"
                        prop:value=move || gen_version.get()
                        on:input=move |ev| set_gen_version.set(event_target_value(&ev))
                    />

                    {move || (changelog_source.get() == "completed_tasks").then(|| {
                        let tasks = selected_tasks.get();
                        view! {
                            <div class="changelog-selected-summary">
                                <p>{format!("{} tasks selected for changelog generation", tasks.len())}</p>
                            </div>
                        }
                    })}

                    {move || (changelog_source.get() != "completed_tasks").then(|| view! {
                        <div>
                            <label class="form-label">"Commits / Changes"</label>
                            <textarea
                                class="form-textarea"
                                rows="8"
                                placeholder="Paste commit messages or describe changes...\ne.g.\n- feat: add changelog page\n- fix: correct nav ordering\n- refactor: clean up API types"
                                prop:value=move || gen_commits.get()
                                on:input=move |ev| set_gen_commits.set(event_target_value(&ev))
                            ></textarea>
                        </div>
                    })}

                    <button
                        class="btn btn-primary btn-lg"
                        on:click=on_generate
                        disabled=move || generating.get()
                    >
                        {move || if generating.get() { "Generating..." } else { "Generate" }}
                    </button>
                </div>
            </div>
        })}

        // Step 3: Release (show generated result)
        {move || (gen_step.get() == 3).then(|| {
            view! {
                <div class="changelog-step-content">
                    {move || generated_result.get().map(|entry| {
                        let version = entry.version.clone();
                        let version_publish = entry.version.clone();
                        let date = entry.date.clone();
                        let sections = entry.sections.clone();
                        let sections_publish = entry.sections.clone();
                        view! {
                            <div class="changelog-preview">
                                <h3>{format!("Generated: v{} ({})", version, date)}</h3>
                                <div class="changelog-changes">
                                    {sections.into_iter().flat_map(|s| {
                                        let cat = s.category.clone();
                                        s.items.into_iter().map(move |item| {
                                            let ccls = category_class(&cat);
                                            let icon = category_icon(&cat);
                                            let cat_display = cat.clone();
                                            view! {
                                                <div class={format!("changelog-change-item {}", ccls)}>
                                                    <span class="changelog-change-icon">{icon}</span>
                                                    <span class="changelog-change-cat">{cat_display}</span>
                                                    <span class="changelog-change-desc">{item}</span>
                                                </div>
                                            }
                                        }).collect::<Vec<_>>()
                                    }).collect::<Vec<_>>()}
                                </div>
                                <div class="changelog-release-actions">
                                    <button
                                        class="btn btn-primary"
                                        disabled=move || publishing.get()
                                        on:click=move |_| {
                                            let ver = version_publish.clone();
                                            let secs = sections_publish.clone();
                                            set_publishing.set(true);
                                            set_error_msg.set(None);
                                            set_success_msg.set(None);
                                            spawn_local(async move {
                                                // Build markdown body from sections
                                                let mut body = String::new();
                                                for s in &secs {
                                                    body.push_str(&format!("## {}\n", s.category));
                                                    for item in &s.items {
                                                        body.push_str(&format!("- {}\n", item));
                                                    }
                                                    body.push('\n');
                                                }
                                                let tag = format!("v{}", ver);
                                                match api::publish_github_release(&tag, &tag, &body).await {
                                                    Ok(release) => {
                                                        let url_info = if release.html_url.is_empty() {
                                                            String::new()
                                                        } else {
                                                            format!(" - {}", release.html_url)
                                                        };
                                                        set_success_msg.set(Some(format!(
                                                            "Published GitHub release {}{}", tag, url_info
                                                        )));
                                                    }
                                                    Err(e) => {
                                                        set_error_msg.set(Some(format!(
                                                            "Failed to publish GitHub release: {e}"
                                                        )));
                                                    }
                                                }
                                                set_publishing.set(false);
                                            });
                                        }
                                    >
                                        {move || if publishing.get() { "Publishing..." } else { "Publish to GitHub Release" }}
                                    </button>
                                    <button class="btn btn-outline" on:click=move |_| {
                                        if entries.get_untracked().is_empty() { set_gen_step.set(1); } else { set_gen_step.set(0); }
                                    }>"Done"</button>
                                </div>
                            </div>
                        }
                    })}
                </div>
            }
        })}

        // Entries list (always visible)
        {move || (gen_step.get() == 0).then(|| view! {
            <div class="changelog-entries">
                <For
                    each=move || entries.get()
                    key=|entry| entry.version.clone()
                    let:entry
                >
                    {
                        let ver = entry.version.clone();
                        let ver_click = entry.version.clone();
                        let is_expanded = move || expanded_version.get().as_deref() == Some(&ver);
                        let date = entry.date.clone();
                        let version_label = entry.version.clone();
                        let sections = entry.sections.clone();
                        let change_count: usize = sections.iter().map(|s| s.items.len()).sum();

                        let grouped = group_sections(&sections);

                        view! {
                            <div
                                class="changelog-entry"
                                on:click=move |_| {
                                    if expanded_version.get().as_deref() == Some(&ver_click) {
                                        set_expanded_version.set(None);
                                    } else {
                                        set_expanded_version.set(Some(ver_click.clone()));
                                    }
                                }
                            >
                                <div class="changelog-entry-header">
                                    <div class="changelog-entry-version">
                                        <span class="changelog-version-tag">{format!("v{}", version_label)}</span>
                                        <span class="changelog-entry-date">{date}</span>
                                    </div>
                                    <span class="changelog-entry-count">
                                        {format!("{} change{}", change_count, if change_count == 1 { "" } else { "s" })}
                                    </span>
                                </div>
                                {move || is_expanded().then(|| {
                                    let grouped = grouped.clone();
                                    view! {
                                        <div class="changelog-entry-details">
                                            {grouped.into_iter().map(|(cat, items)| {
                                                let ccls = category_class(&cat);
                                                let cat_display = cat.clone();
                                                view! {
                                                    <div class="changelog-category-group">
                                                        <h4 class={format!("changelog-category-title {}", ccls)}>
                                                            {cat_display}
                                                        </h4>
                                                        <ul class="changelog-change-list">
                                                            {items.into_iter().map(|desc| view! {
                                                                <li class="changelog-change-li">{desc}</li>
                                                            }).collect::<Vec<_>>()}
                                                        </ul>
                                                    </div>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    }
                                })}
                            </div>
                        }
                    }
                </For>
            </div>
        })}

        {move || (!loading.get() && entries.get().is_empty() && error_msg.get().is_none() && gen_step.get() == 0).then(|| view! {
            <div class="state-empty state-empty-compact">
                <div
                    class="state-empty-icon"
                    inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="8" y1="13" x2="16" y2="13"/><line x1="8" y1="17" x2="14" y2="17"/></svg>"#
                ></div>
                <div class="state-empty-title">"No changelog entries yet"</div>
                <div class="state-empty-hint">"Generate a changelog from completed tasks or git history."</div>
            </div>
        })}
    }
}

fn default_version() -> String {
    let now = chrono::Utc::now();
    now.format("%Y.%m.%d").to_string()
}

/// Group sections by category, preserving order of first appearance.
fn group_sections(sections: &[api::ApiChangelogSection]) -> Vec<(String, Vec<String>)> {
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for section in sections {
        let cat = section.category.clone();
        if let Some(group) = groups.iter_mut().find(|(c, _)| *c == cat) {
            group.1.extend(section.items.clone());
        } else {
            groups.push((cat, section.items.clone()));
        }
    }
    groups
}
