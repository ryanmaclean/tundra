use leptos::prelude::*;

use crate::state::use_app_state;

#[component]
pub fn WorktreesPage() -> impl IntoView {
    let state = use_app_state();
    let worktrees = state.worktrees;

    view! {
        <div class="page-header">
            <h2>"Worktrees"</h2>
        </div>

        <div class="section">
            <p class="section-description">
                "Active git worktrees managed by auto-tundra agents."
            </p>
        </div>

        <table class="data-table">
            <thead>
                <tr>
                    <th>"Branch"</th>
                    <th>"Path"</th>
                    <th>"Status"</th>
                    <th>"Last Commit"</th>
                </tr>
            </thead>
            <tbody>
                {move || worktrees.get().into_iter().map(|wt| {
                    let status_class = match wt.status.as_str() {
                        "active" => "glyph-active",
                        "stale" => "glyph-stopped",
                        _ => "glyph-unknown",
                    };
                    view! {
                        <tr>
                            <td><code class="branch-name">{wt.branch.clone()}</code></td>
                            <td><code class="file-path">{wt.path.clone()}</code></td>
                            <td><span class={status_class}>{wt.status.clone()}</span></td>
                            <td class="commit-msg">{wt.last_commit.clone()}</td>
                        </tr>
                    }
                }).collect::<Vec<_>>()}
            </tbody>
        </table>
    }
}
