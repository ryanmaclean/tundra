use leptos::prelude::*;

use crate::state::use_app_state;

#[component]
pub fn McpPage() -> impl IntoView {
    let state = use_app_state();
    let servers = state.mcp_servers;

    view! {
        <div class="page-header">
            <h2>"MCP Servers"</h2>
        </div>
        <table class="data-table">
            <thead>
                <tr>
                    <th>"Name"</th>
                    <th>"Endpoint"</th>
                    <th>"Status"</th>
                    <th>"Tools"</th>
                </tr>
            </thead>
            <tbody>
                {move || servers.get().into_iter().map(|s| {
                    let status_class = if s.status == "connected" {
                        "glyph-active"
                    } else {
                        "glyph-stopped"
                    };
                    view! {
                        <tr>
                            <td>{s.name}</td>
                            <td>{s.endpoint}</td>
                            <td><span class={status_class}>{s.status}</span></td>
                            <td>{s.tools_count}</td>
                        </tr>
                    }
                }).collect::<Vec<_>>()}
            </tbody>
        </table>
    }
}
