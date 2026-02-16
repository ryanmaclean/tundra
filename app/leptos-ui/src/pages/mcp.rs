use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;

#[component]
pub fn McpPage() -> impl IntoView {
    let (servers, set_servers) = signal(Vec::<api::ApiMcpServer>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (endpoint_missing, set_endpoint_missing) = signal(false);

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        set_endpoint_missing.set(false);
        spawn_local(async move {
            match api::fetch_mcp_servers().await {
                Ok(data) => set_servers.set(data),
                Err(e) => {
                    if e.contains("404") || e.contains("Not Found") {
                        set_endpoint_missing.set(true);
                    } else {
                        set_error_msg.set(Some(format!("Failed to fetch MCP servers: {e}")));
                    }
                }
            }
            set_loading.set(false);
        });
    };

    do_refresh();

    view! {
        <div class="page-header">
            <h2>"MCP Overview"</h2>
            <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                "\u{21BB} Refresh"
            </button>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">"Loading MCP servers..."</div>
        })}

        {move || endpoint_missing.get().then(|| view! {
            <div class="agent-card" style="margin: 24px auto; max-width: 480px; text-align: center; padding: 32px;">
                <h3 style="margin-bottom: 8px;">"Coming Soon"</h3>
                <p style="color: #8b949e; margin-bottom: 16px;">
                    "MCP server management is not yet available. This feature will show connected MCP servers, their status, and available tools."
                </p>
                <div style="font-size: 2em; margin-bottom: 12px;">"--"</div>
                <span class="glyph-pending">"Planned"</span>
            </div>
        })}

        {move || (!endpoint_missing.get() && !loading.get()).then(|| view! {
            <table class="data-table">
                <thead>
                    <tr>
                        <th>"Name"</th>
                        <th>"Status"</th>
                        <th>"Connected Tools"</th>
                    </tr>
                </thead>
                <tbody>
                    {move || servers.get().into_iter().map(|s| {
                        let status_class = if s.status == "connected" || s.status == "active" {
                            "glyph-active"
                        } else {
                            "glyph-stopped"
                        };
                        let tools_str = if s.tools.is_empty() {
                            "None".to_string()
                        } else {
                            s.tools.join(", ")
                        };
                        view! {
                            <tr>
                                <td><strong>{s.name}</strong></td>
                                <td><span class={status_class}>{s.status}</span></td>
                                <td style="font-size: 0.85em;">{tools_str}</td>
                            </tr>
                        }
                    }).collect::<Vec<_>>()}
                </tbody>
            </table>
        })}

        {move || (!loading.get() && !endpoint_missing.get() && servers.get().is_empty() && error_msg.get().is_none()).then(|| view! {
            <div class="dashboard-loading">"No MCP servers found."</div>
        })}
    }
}
