use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;

#[component]
pub fn AgentsPage() -> impl IntoView {
    let (agents, set_agents) = signal(Vec::<api::ApiAgent>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::fetch_agents().await {
                Ok(data) => set_agents.set(data),
                Err(e) => set_error_msg.set(Some(format!("Failed to fetch agents: {e}"))),
            }
            set_loading.set(false);
        });
    };

    // Initial fetch
    do_refresh();

    let stop_agent = move |id: String| {
        spawn_local(async move {
            match api::stop_agent(&id).await {
                Ok(_) => {
                    // Refresh list after stopping
                    match api::fetch_agents().await {
                        Ok(data) => set_agents.set(data),
                        Err(_) => {}
                    }
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("Failed to stop agent: {e}").into());
                }
            }
        });
    };

    let status_badge_class = |status: &str| -> &'static str {
        match status {
            "active" | "running" => "glyph-active",
            "idle" => "glyph-idle",
            "pending" | "starting" => "glyph-pending",
            "stopped" | "dead" => "glyph-stopped",
            _ => "glyph-unknown",
        }
    };

    view! {
        <div class="page-header">
            <h2>"Agents"</h2>
            <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                "\u{21BB} Refresh"
            </button>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">"Loading agents..."</div>
        })}

        <table class="data-table">
            <thead>
                <tr>
                    <th>"Name"</th>
                    <th>"Role"</th>
                    <th>"Status"</th>
                    <th>"ID"</th>
                    <th>"Actions"</th>
                </tr>
            </thead>
            <tbody>
                {move || agents.get().into_iter().map(|agent| {
                    let id = agent.id.clone();
                    let id_stop = agent.id.clone();
                    let status = agent.status.clone();
                    let badge_cls = status_badge_class(&status);
                    let stop_agent = stop_agent.clone();
                    let is_active = status == "active" || status == "running" || status == "idle";
                    view! {
                        <tr>
                            <td><strong>{agent.name}</strong></td>
                            <td>{agent.role}</td>
                            <td><span class={badge_cls}>{status}</span></td>
                            <td><code>{id}</code></td>
                            <td>
                                {is_active.then(|| {
                                    let stop = stop_agent.clone();
                                    view! {
                                        <button
                                            class="action-btn action-recover"
                                            on:click=move |_| stop(id_stop.clone())
                                        >
                                            "Stop"
                                        </button>
                                    }
                                })}
                            </td>
                        </tr>
                    }
                }).collect::<Vec<_>>()}
            </tbody>
        </table>

        {move || (!loading.get() && agents.get().is_empty() && error_msg.get().is_none()).then(|| view! {
            <div class="dashboard-loading">"No agents found."</div>
        })}
    }
}
