use leptos::prelude::*;
use crate::state::use_app_state;
use crate::themed::{themed, Prompt};
use leptos::task::spawn_local;

use crate::api;

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

#[component]
pub fn WorktreesPage() -> impl IntoView {
    let app_state = use_app_state();
    let display_mode = app_state.display_mode;
    let (worktrees, set_worktrees) = signal(Vec::<WorktreeDisplay>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (selected_worktrees, set_selected_worktrees) = signal(std::collections::HashSet::<String>::new());
    let (status_msg, set_status_msg) = signal(Option::<String>::None);

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::fetch_worktrees().await {
                Ok(data) => {
                    let display: Vec<WorktreeDisplay> = data.into_iter()
                        .map(WorktreeDisplay::from_api)
                        .collect();
                    set_worktrees.set(display);
                }
                Err(e) => {
                    if e.contains("404") || e.contains("Not Found") {
                        set_worktrees.set(Vec::new());
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
                Ok(_) => {
                    match api::fetch_worktrees().await {
                        Ok(data) => {
                            let display: Vec<WorktreeDisplay> = data.into_iter()
                                .map(WorktreeDisplay::from_api)
                                .collect();
                            set_worktrees.set(display);
                        }
                        Err(_) => {}
                    }
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("Failed to delete worktree: {e}").into());
                }
            }
        });
    };

    let worktree_count = move || worktrees.get().len();

    view! {
        <div class="page-header" style="border-bottom: none; flex-wrap: wrap; gap: 8px;">
            <div>
                <h2 style="display: flex; align-items: center; gap: 8px;">
                    "\u{1F333} Worktrees"
                    <span class="worktree-count-badge">{move || format!("{} Total Worktrees", worktree_count())}</span>
                </h2>
                <span class="worktree-header-desc">"Manage isolated workspaces for your Auto Claude tasks"</span>
            </div>
            <div class="page-header-actions" style="margin-left: auto;">
                <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                    "\u{21BB} Refresh"
                </button>
            </div>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error" style="margin: 0 16px 8px;">{msg}</div>
        })}

        {move || status_msg.get().map(|msg| view! {
            <div class="pr-status-banner" style="margin: 0 16px 8px;">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading" style="padding: 0 16px;">{move || themed(display_mode.get(), Prompt::Loading)}</div>
        })}

        // Worktree cards
        <div class="worktree-cards">
            {move || worktrees.get().into_iter().map(|wt| {
                let id = wt.inner.id.clone();
                let id_merge = id.clone();
                let id_cleanup = id.clone();
                let id_done = id.clone();
                let id_checkbox = id.clone();
                let delete = delete_worktree.clone();
                let delete_done = delete_worktree.clone();
                let bead_display = if wt.inner.bead_id.is_empty() { "--".to_string() } else { wt.inner.bead_id.clone() };
                let status_class = match wt.inner.status.as_str() {
                    "active" => "glyph-active",
                    "stale" => "glyph-stopped",
                    _ => "glyph-unknown",
                };
                let branch = wt.inner.branch.clone();
                let wt_path = wt.inner.path.clone();
                let is_checked = move || selected_worktrees.get().contains(&id_checkbox);

                view! {
                    <div class="worktree-card">
                        <div class="worktree-card-top">
                            <div class="worktree-branch-row">
                                <input
                                    type="checkbox"
                                    class="worktree-checkbox"
                                    prop:checked=move || is_checked()
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
                                <span class={status_class} style="font-size: 10px;">"\u{25CF} "</span>
                                <span class="worktree-branch-name">{branch}</span>
                            </div>
                            <span class="worktree-bead-link">{bead_display}</span>
                        </div>
                        <div class="worktree-stats">
                            <span class="worktree-stat-item">{format!("Path: {}", wt_path)}</span>
                            <span>"\u{2022}"</span>
                            <span class="worktree-stat-item">{format!("Status: {}", wt.inner.status)}</span>
                        </div>
                        <div class="worktree-actions">
                            <button class="wt-btn wt-btn-merge" on:click=move |_| {
                                let merge_id = id_merge.clone();
                                set_status_msg.set(Some(format!("Merging worktree {}...", merge_id)));
                                spawn_local(async move {
                                    match api::merge_worktree(&merge_id).await {
                                        Ok(_) => {
                                            set_status_msg.set(Some("Merge completed successfully".to_string()));
                                            // Refresh worktrees list
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
                            <button
                                class="wt-btn wt-btn-cleanup"
                                on:click=move |_| delete(id_cleanup.clone())
                            >"Cleanup"</button>
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
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>

        {move || (!loading.get() && worktrees.get().is_empty() && error_msg.get().is_none()).then(|| view! {
            <div class="worktree-empty">
                <div class="worktree-empty-icon">"\u{1F333}"</div>
                <div class="worktree-empty-text">{move || themed(display_mode.get(), Prompt::EmptyKpi)}</div>
                <div class="worktree-empty-hint">"Worktrees are created when agents start working on tasks."</div>
            </div>
        })}
    }
}
