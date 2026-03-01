use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::components::spinner::Spinner;
use crate::i18n::t;

#[component]
pub fn SessionsPage() -> impl IntoView {
    let (sessions, set_sessions) = signal(Vec::<api::ApiSession>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::fetch_sessions().await {
                Ok(data) => set_sessions.set(data),
                Err(e) => set_error_msg.set(Some(format!("Failed to fetch sessions: {e}"))),
            }
            set_loading.set(false);
        });
    };

    do_refresh();

    let status_class = |status: &str| -> &'static str {
        match status {
            "active" | "running" => "glyph-active",
            "idle" => "glyph-idle",
            "stopped" | "completed" => "glyph-stopped",
            _ => "glyph-unknown",
        }
    };

    view! {
        <div class="page-header">
            <h2>{t("nav-sessions")}</h2>
            <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                {format!("\u{21BB} {}", t("btn-refresh"))}
            </button>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <Spinner size="md" label=""/>
        })}

        <table class="data-table">
            <thead>
                <tr>
                    <th>"Session ID"</th>
                    <th>"Agent"</th>
                    <th>"CLI Type"</th>
                    <th>"Status"</th>
                    <th>"Duration"</th>
                    <th>"Actions"</th>
                </tr>
            </thead>
            <tbody>
                {move || sessions.get().into_iter().map(|s| {
                    let status = s.status.clone();
                    let scls = status_class(&status);
                    let is_active = status == "active" || status == "running";
                    let _sid = s.id.clone();
                    view! {
                        <tr>
                            <td><code>{s.id}</code></td>
                            <td>{s.agent_name}</td>
                            <td>{s.cli_type}</td>
                            <td><span class={scls}>{status}</span></td>
                            <td>{s.duration}</td>
                            <td>
                                {is_active.then(move || view! {
                                    <button
                                        class="action-btn action-start"
                                        on:click=move |_| {
                                            if let Some(window) = web_sys::window() {
                                                let _ = window.location().set_hash("terminals");
                                            }
                                        }
                                    >
                                        "View Terminal"
                                    </button>
                                })}
                            </td>
                        </tr>
                    }
                }).collect::<Vec<_>>()}
            </tbody>
        </table>

        {move || (!loading.get() && sessions.get().is_empty() && error_msg.get().is_none()).then(|| view! {
            <div class="dashboard-loading">{t("status-empty")}</div>
        })}
    }
}
