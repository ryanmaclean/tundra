use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;

#[component]
pub fn WorktreesPage() -> impl IntoView {
    let (worktrees, set_worktrees) = signal(Vec::<api::ApiWorktree>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::fetch_worktrees().await {
                Ok(data) => set_worktrees.set(data),
                Err(e) => set_error_msg.set(Some(format!("Failed to fetch worktrees: {e}"))),
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
                        Ok(data) => set_worktrees.set(data),
                        Err(_) => {}
                    }
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("Failed to delete worktree: {e}").into());
                }
            }
        });
    };

    view! {
        <div class="page-header">
            <h2>"Worktrees"</h2>
            <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                "\u{21BB} Refresh"
            </button>
        </div>

        <div class="section">
            <p class="section-description">
                "Active git worktrees managed by auto-tundra agents."
            </p>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">"Loading worktrees..."</div>
        })}

        <table class="data-table">
            <thead>
                <tr>
                    <th>"Branch"</th>
                    <th>"Path"</th>
                    <th>"Bead"</th>
                    <th>"Status"</th>
                    <th>"Actions"</th>
                </tr>
            </thead>
            <tbody>
                {move || worktrees.get().into_iter().map(|wt| {
                    let id = wt.id.clone();
                    let delete = delete_worktree.clone();
                    let status_class = match wt.status.as_str() {
                        "active" => "glyph-active",
                        "stale" => "glyph-stopped",
                        _ => "glyph-unknown",
                    };
                    view! {
                        <tr>
                            <td><code class="branch-name">{wt.branch}</code></td>
                            <td><code class="file-path">{wt.path}</code></td>
                            <td><code>{wt.bead_id}</code></td>
                            <td><span class={status_class}>{wt.status}</span></td>
                            <td>
                                <button
                                    class="action-btn action-recover"
                                    on:click=move |_| delete(id.clone())
                                >
                                    "Delete"
                                </button>
                            </td>
                        </tr>
                    }
                }).collect::<Vec<_>>()}
            </tbody>
        </table>

        {move || (!loading.get() && worktrees.get().is_empty() && error_msg.get().is_none()).then(|| view! {
            <div class="dashboard-loading">"No worktrees found."</div>
        })}
    }
}
