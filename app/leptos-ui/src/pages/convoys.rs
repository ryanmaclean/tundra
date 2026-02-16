use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;

#[component]
pub fn ConvoysPage() -> impl IntoView {
    let (convoys, set_convoys) = signal(Vec::<api::ApiConvoy>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (endpoint_missing, set_endpoint_missing) = signal(false);

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        set_endpoint_missing.set(false);
        spawn_local(async move {
            match api::fetch_convoys().await {
                Ok(data) => set_convoys.set(data),
                Err(e) => {
                    if e.contains("404") || e.contains("Not Found") {
                        set_endpoint_missing.set(true);
                    } else {
                        set_error_msg.set(Some(format!("Failed to fetch convoys: {e}")));
                    }
                }
            }
            set_loading.set(false);
        });
    };

    do_refresh();

    view! {
        <div class="page-header">
            <h2>"Convoys"</h2>
            <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                "\u{21BB} Refresh"
            </button>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">"Loading convoys..."</div>
        })}

        {move || endpoint_missing.get().then(|| view! {
            <div class="agent-card" style="margin: 24px auto; max-width: 480px; text-align: center; padding: 32px;">
                <h3 style="margin-bottom: 8px;">"Coming Soon"</h3>
                <p style="color: #8b949e; margin-bottom: 16px;">
                    "Convoy orchestration is not yet available. This feature will allow coordinating multiple agents working on related tasks with dependency tracking."
                </p>
                <div style="font-size: 2em; margin-bottom: 12px;">"--"</div>
                <span class="glyph-pending">"Planned"</span>
            </div>
        })}

        {move || (!endpoint_missing.get() && !loading.get()).then(|| view! {
            <div class="section">
                {move || convoys.get().into_iter().map(|c| {
                    let status_class = match c.status.as_str() {
                        "active" | "running" => "glyph-active",
                        "pending" => "glyph-pending",
                        "completed" | "done" => "glyph-idle",
                        _ => "glyph-unknown",
                    };
                    view! {
                        <div class="agent-card" style="margin-bottom: 12px;">
                            <div class="agent-header">
                                <span class="agent-name">{c.name}</span>
                                <span class="agent-role">{format!("{} beads", c.bead_count)}</span>
                            </div>
                            <div style="margin-top: 4px; font-size: 0.85em; color: #8b949e;">
                                <span>{format!("ID: {}", c.id)}</span>
                                " - "
                                <span class={status_class}>{c.status}</span>
                            </div>
                        </div>
                    }
                }).collect::<Vec<_>>()}
            </div>
        })}

        {move || (!loading.get() && !endpoint_missing.get() && convoys.get().is_empty() && error_msg.get().is_none()).then(|| view! {
            <div class="dashboard-loading">"No convoys found."</div>
        })}
    }
}
