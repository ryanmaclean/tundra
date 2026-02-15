use leptos::prelude::*;

use crate::state::use_app_state;

#[component]
pub fn SessionsPage() -> impl IntoView {
    let state = use_app_state();
    let sessions = state.sessions;

    view! {
        <div class="page-header">
            <h2>"Sessions"</h2>
        </div>
        <table class="data-table">
            <thead>
                <tr>
                    <th>"ID"</th>
                    <th>"Name"</th>
                    <th>"Started"</th>
                    <th>"Agents"</th>
                    <th>"Beads"</th>
                    <th>"Status"</th>
                </tr>
            </thead>
            <tbody>
                {move || sessions.get().into_iter().map(|s| {
                    view! {
                        <tr>
                            <td>{s.id}</td>
                            <td>{s.name}</td>
                            <td>{s.started_at}</td>
                            <td>{s.agent_count}</td>
                            <td>{s.bead_count}</td>
                            <td>{s.status}</td>
                        </tr>
                    }
                }).collect::<Vec<_>>()}
            </tbody>
        </table>
    }
}
