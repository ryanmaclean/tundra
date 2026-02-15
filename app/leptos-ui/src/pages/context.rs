use leptos::prelude::*;

use crate::state::use_app_state;

#[component]
pub fn ContextPage() -> impl IntoView {
    let state = use_app_state();
    let entries = state.context_entries;

    view! {
        <div class="page-header">
            <h2>"Context"</h2>
        </div>

        <div class="section">
            <p class="section-description">
                "Project context files and documentation used by agents."
            </p>
        </div>

        <table class="data-table">
            <thead>
                <tr>
                    <th>"File Path"</th>
                    <th>"Description"</th>
                    <th>"Last Modified"</th>
                </tr>
            </thead>
            <tbody>
                {move || entries.get().into_iter().map(|entry| {
                    view! {
                        <tr>
                            <td><code class="file-path">{entry.path.clone()}</code></td>
                            <td>{entry.description.clone()}</td>
                            <td class="timestamp-cell">{entry.last_modified.clone()}</td>
                        </tr>
                    }
                }).collect::<Vec<_>>()}
            </tbody>
        </table>
    }
}
