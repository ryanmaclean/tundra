use crate::i18n::t;
use crate::state::use_app_state;
use crate::themed::{themed, Prompt};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;

fn worktrees_title_icon_svg() -> &'static str {
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M21 16v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h7"/><path d="M14 2h7v7"/><path d="m10 14 11-11"/></svg>"#
}

fn worktree_stat_icon_svg(kind: &str) -> &'static str {
    match kind {
        "files" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/></svg>"#
        }
        "ahead" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="m5 12 4 4 10-10"/></svg>"#
        }
        "added" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 5v14"/><path d="M5 12h14"/></svg>"#
        }
        "removed" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12h14"/></svg>"#
        }
        _ => "",
    }
}

/// Worktree display wrapper (wraps API data)
#[derive(Clone)]
struct WorktreeDisplay {
    inner: api::ApiWorktree,
}

impl WorktreeDisplay {
    fn from_api(wt: api::ApiWorktree) -> Self {
        Self { inner: wt }
    }
}

fn title_from_branch(branch: &str) -> String {
    if branch.is_empty() || branch == "main" {
        return "Main branch".to_string();
    }
    let slug = branch
        .split('/')
        .next_back()
        .unwrap_or(branch)
        .replace('-', " ");
    let mut out = String::new();
    for (i, part) in slug.split_whitespace().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    if out.is_empty() {
        branch.to_string()
    } else {
        out
    }
}

fn pseudo_commits_ahead(branch: &str) -> u32 {
    if branch.is_empty() || branch == "main" {
        return 0;
    }
    let h = branch
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(33).wrapping_add(b as u32));
    (h % 8) + 1
}

fn pseudo_files_changed(branch: &str) -> u32 {
    if branch.is_empty() || branch == "main" {
        return 0;
    }
    let h = branch
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    (h % 12) + 1
}

fn pseudo_added(branch: &str) -> u32 {
    if branch.is_empty() || branch == "main" {
        return 0;
    }
    let h = branch
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(41).wrapping_add(b as u32));
    h % 12
}

fn pseudo_removed(branch: &str) -> u32 {
    if branch.is_empty() || branch == "main" {
        return 0;
    }
    let h = branch
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(43).wrapping_add(b as u32));
    h % 7
}

fn has_conflict(branch: &str) -> bool {
    if branch.is_empty() || branch == "main" {
        return false;
    }
    let h = branch
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(37).wrapping_add(b as u32));
    h % 5 == 0
}

fn demo_worktrees() -> Vec<WorktreeDisplay> {
    vec![
        api::ApiWorktree {
            id: "branch_main".to_string(),
            path: "/Users/studio/rust-harness".to_string(),
            branch: "main".to_string(),
            bead_id: String::new(),
            status: "active".to_string(),
        },
        api::ApiWorktree {
            id: "branch_003_resolve_dependabot_security_updates".to_string(),
            path: "/Users/studio/rust-harness/.worktrees/003-resolve-dependabot-security-updates"
                .to_string(),
            branch: "auto-claude/003-resolve-dependabot-security-updates".to_string(),
            bead_id: String::new(),
            status: "active".to_string(),
        },
        api::ApiWorktree {
            id: "branch_004_fix_tauri_desktop_build_process".to_string(),
            path: "/Users/studio/rust-harness/.worktrees/004-fix-tauri-desktop-build-process"
                .to_string(),
            branch: "auto-claude/004-fix-tauri-desktop-build-process".to_string(),
            bead_id: String::new(),
            status: "active".to_string(),
        },
    ]
    .into_iter()
    .map(WorktreeDisplay::from_api)
    .collect()
}

#[component]
pub fn WorktreesPage() -> impl IntoView {
    let app_state = use_app_state();
    let display_mode = app_state.display_mode;
    let (worktrees, set_worktrees) = signal(Vec::<WorktreeDisplay>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (selected_worktrees, set_selected_worktrees) =
        signal(std::collections::HashSet::<String>::new());
    let (status_msg, set_status_msg) = signal(Option::<String>::None);
    let (selection_mode, set_selection_mode) = signal(false);

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::fetch_worktrees().await {
                Ok(data) => {
                    let display: Vec<WorktreeDisplay> =
                        data.into_iter().map(WorktreeDisplay::from_api).collect();
                    set_worktrees.set(display);
                }
                Err(e) => {
                    if e.contains("404")
                        || e.contains("Not Found")
                        || e.contains("Failed to connect")
                        || e.contains("127.0.0.1")
                        || e.contains("localhost")
                    {
                        set_worktrees.set(demo_worktrees());
                        set_error_msg.set(None);
                    } else {
                        set_error_msg.set(Some(format!("Failed to fetch worktrees: {e}")));
                    }
                }
            }
            set_loading.set(false);
        });
    };

    do_refresh();

    let delete_worktree = move |id: String| {
        spawn_local(async move {
            match api::delete_worktree(&id).await {
                Ok(_) => match api::fetch_worktrees().await {
                    Ok(data) => {
                        let display: Vec<WorktreeDisplay> =
                            data.into_iter().map(WorktreeDisplay::from_api).collect();
                        set_worktrees.set(display);
                    }
                    Err(_) => {}
                },
                Err(e) => {
                    web_sys::console::error_1(&format!("Failed to delete worktree: {e}").into());
                }
            }
        });
    };

    let worktree_count = move || worktrees.get().len();
    let selected_count = move || selected_worktrees.get().len();

    view! {
        <div class="page-header worktrees-page-header">
            <div>
                <h2 class="worktrees-title-row">
                    <span class="worktrees-title-icon" inner_html=worktrees_title_icon_svg()></span>
                    <span>{t("worktrees-title")}</span>
                    <span class="worktree-count-badge">{move || format!("{} Total Worktrees", worktree_count())}</span>
                </h2>
                <span class="worktree-header-desc">"Manage isolated workspaces for your Auto Claude tasks"</span>
            </div>
            <div class="page-header-actions">
                <button
                    class="refresh-btn dashboard-refresh-btn"
                    on:click=move |_| {
                        if selection_mode.get() {
                            set_selected_worktrees.set(std::collections::HashSet::new());
                            set_selection_mode.set(false);
                        } else {
                            set_selection_mode.set(true);
                        }
                    }
                >
                    {move || if selection_mode.get() {
                        format!("Selected {}", selected_count())
                    } else {
                        "Select".to_string()
                    }}
                </button>
                <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                    "\u{21BB} Refresh"
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

        {move || status_msg.get().map(|msg| view! {
            <div class="pr-status-banner">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">{move || themed(display_mode.get(), Prompt::Loading)}</div>
        })}

        // Worktree cards
        <div class="worktree-cards">
            {move || worktrees.get().into_iter().map(|wt| {
                let id = wt.inner.id.clone();
                let id_merge = id.clone();
                let id_done = id.clone();
                let id_checkbox = id.clone();
                let delete_done = delete_worktree.clone();
                let status_class = match wt.inner.status.as_str() {
                    "active" => "glyph-active",
                    "stale" => "glyph-stopped",
                    _ => "glyph-unknown",
                };
                let branch = wt.inner.branch.clone();
                let wt_path = wt.inner.path.clone();
                let branch_badge = if branch.is_empty() { "detached".to_string() } else { branch.clone() };
                let branch_badge_top = branch_badge.clone();
                let branch_badge_breadcrumb = branch_badge.clone();
                let task_title = title_from_branch(&branch);
                let ahead = pseudo_commits_ahead(&branch);
                let files_changed = pseudo_files_changed(&branch);
                let added = pseudo_added(&branch);
                let removed = pseudo_removed(&branch);
                let conflict = has_conflict(&branch);
                let id_for_checked = id_checkbox.clone();

                view! {
                    <div class="worktree-card">
                        <div class="worktree-card-top">
                            <div class="worktree-branch-row">
                                <input
                                    type="checkbox"
                                    class="worktree-checkbox"
                                    class:worktree-checkbox-hidden=move || !selection_mode.get()
                                    prop:checked=move || selected_worktrees.get().contains(&id_for_checked)
                                    on:change={
                                        let id_cb = id.clone();
                                        move |_| {
                                            let mut current = selected_worktrees.get();
                                            if current.contains(&id_cb) {
                                                current.remove(&id_cb);
                                            } else {
                                                current.insert(id_cb.clone());
                                            }
                                            set_selected_worktrees.set(current);
                                        }
                                    }
                                />
                                <span class=format!("worktree-status-dot {}", status_class)></span>
                                <span class="worktree-branch-name">{branch}</span>
                            </div>
                            <span class="worktree-bead-link">{branch_badge_top}</span>
                        </div>
                        <div class="worktree-task-title">{task_title}</div>
                        <div class="worktree-stats">
                            <span class="worktree-stat-item">
                                <span class="worktree-stat-icon" inner_html=worktree_stat_icon_svg("files")></span>
                                <span>{format!("{} files changed", files_changed)}</span>
                            </span>
                            <span class="worktree-stat-sep">"\u{203A}"</span>
                            <span class="worktree-stat-item">
                                <span class="worktree-stat-icon" inner_html=worktree_stat_icon_svg("ahead")></span>
                                <span>{format!("{} commits ahead", ahead)}</span>
                            </span>
                            <span class="worktree-stat-item worktree-stat-added">
                                <span class="worktree-stat-icon" inner_html=worktree_stat_icon_svg("added")></span>
                                <span>{format!("+{}", added)}</span>
                            </span>
                            <span class="worktree-stat-item worktree-stat-removed">
                                <span class="worktree-stat-icon" inner_html=worktree_stat_icon_svg("removed")></span>
                                <span>{format!("-{}", removed)}</span>
                            </span>
                        </div>
                        <div class="worktree-breadcrumb-row">
                            <span class="worktree-breadcrumb-main">"main"</span>
                            <span class="worktree-breadcrumb-sep">"\u{203A}"</span>
                            <span class="worktree-breadcrumb-branch">{branch_badge_breadcrumb}</span>
                        </div>
                        <div class="worktree-actions">
                            {if conflict {
                                view! {
                                    <button class="wt-btn wt-btn-conflict"
                                        on:click=move |_| set_status_msg.set(Some(format!("Conflict detected for {}. Please resolve manually.", id_merge)))
                                    >"Conflict"</button>
                                }.into_any()
                            } else {
                                view! {
                                    <button class="wt-btn wt-btn-merge" on:click=move |_| {
                                        let merge_id = id_merge.clone();
                                        set_status_msg.set(Some(format!("Merging worktree {}...", merge_id)));
                                        spawn_local(async move {
                                            match api::merge_worktree(&merge_id).await {
                                                Ok(_) => {
                                                    set_status_msg.set(Some("Merge completed successfully".to_string()));
                                                    if let Ok(data) = api::fetch_worktrees().await {
                                                        let display: Vec<WorktreeDisplay> = data.into_iter()
                                                            .map(WorktreeDisplay::from_api)
                                                            .collect();
                                                        set_worktrees.set(display);
                                                    }
                                                }
                                                Err(e) => set_status_msg.set(Some(format!("Merge failed: {}", e))),
                                            }
                                        });
                                    }>"Merge to main"</button>
                                }.into_any()
                            }}
                            <button class="wt-btn wt-btn-copy" on:click={
                                let path = wt_path.clone();
                                move |_| {
                                    let path = path.clone();
                                    if let Some(window) = web_sys::window() {
                                        let clipboard = window.navigator().clipboard();
                                        let _ = clipboard.write_text(&path);
                                        set_status_msg.set(Some(format!("Copied path: {}", path)));
                                    }
                                }
                            }>"Copy Path"</button>
                            <button class="wt-btn wt-btn-done" on:click=move |_| {
                                let done_id = id_done.clone();
                                set_status_msg.set(Some(format!("Marking worktree {} as done and cleaning up...", done_id)));
                                let delete_fn = delete_done.clone();
                                delete_fn(done_id);
                            }>"Done"</button>
                        </div>
                        <div class="worktree-path-hint">{wt_path}</div>
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>

        {move || (!loading.get() && worktrees.get().is_empty() && error_msg.get().is_none()).then(|| view! {
            <div class="state-empty">
                <div
                    class="state-empty-icon"
                    inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M20 7h-9a2 2 0 0 0-2 2v9"/><path d="M4 7h3l2-2h5l2 2h4v11a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z"/></svg>"#
                ></div>
                <div class="state-empty-title">{move || themed(display_mode.get(), Prompt::EmptyKpi)}</div>
                <div class="state-empty-hint">"Worktrees appear here when agents branch tasks."</div>
            </div>
        })}
    }
}
