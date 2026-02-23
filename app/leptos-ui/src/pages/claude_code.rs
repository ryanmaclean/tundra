use crate::state::use_app_state;
use crate::themed::{themed, Prompt};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;

#[component]
pub fn ClaudeCodePage() -> impl IntoView {
    let app_state = use_app_state();
    let display_mode = app_state.display_mode;
    let (agents, set_agents) = signal(Vec::<api::ApiAgent>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (selected_agent, set_selected_agent) = signal(Option::<api::ApiAgent>::None);

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::fetch_agents().await {
                Ok(data) => {
                    // Filter to only claude_code role agents
                    let filtered: Vec<_> = data
                        .into_iter()
                        .filter(|a| {
                            a.role == "claude_code"
                                || a.role == "claude-code"
                                || a.role.contains("claude")
                        })
                        .collect();
                    set_agents.set(filtered);
                }
                Err(e) => set_error_msg.set(Some(format!("Failed to fetch agents: {e}"))),
            }
            set_loading.set(false);
        });
    };

    do_refresh();

    let active_count = move || {
        agents
            .get()
            .iter()
            .filter(|a| a.status == "active" || a.status == "running")
            .count()
    };
    let total_count = move || agents.get().len();

    let integration_status = move || {
        if total_count() > 0 {
            "Connected"
        } else {
            "No Claude Code agents"
        }
    };
    let integration_class = move || {
        if total_count() > 0 {
            "glyph-active"
        } else {
            "glyph-stopped"
        }
    };

    view! {
        <div class="page-header">
            <h2>"Claude Code"</h2>
            <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                "\u{21BB} Refresh"
            </button>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">{move || themed(display_mode.get(), Prompt::Loading)}</div>
        })}

        <div class="section">
            <p class="section-description">
                "Active Claude Code sessions managed by auto-tundra agents."
            </p>
        </div>

        <div class="kpi-grid" style="grid-template-columns: repeat(3, 1fr);">
            <div class="kpi-card">
                <div class="value">
                    <span class={integration_class}>{integration_status}</span>
                </div>
                <div class="label">"Integration Status"</div>
            </div>
            <div class="kpi-card">
                <div class="value">{active_count}</div>
                <div class="label">"Active Sessions"</div>
            </div>
            <div class="kpi-card">
                <div class="value">{total_count}</div>
                <div class="label">"Total Claude Agents"</div>
            </div>
        </div>

        <div class="session-grid">
            {move || agents.get().into_iter().map(|agent| {
                let status = agent.status.clone();
                let status_class = match status.as_str() {
                    "active" | "running" => "session-active",
                    "idle" => "session-idle",
                    "stopped" | "dead" => "session-stopped",
                    _ => "session-unknown",
                };
                let glyph_class = match status.as_str() {
                    "active" | "running" => "glyph-active",
                    "idle" => "glyph-idle",
                    "stopped" | "dead" => "glyph-stopped",
                    _ => "glyph-unknown",
                };
                view! {
                    <div class={format!("session-card {}", status_class)}>
                        <div class="session-card-header">
                            <span class="session-name">{agent.name.clone()}</span>
                            <span class={glyph_class}>{status}</span>
                        </div>
                        <div class="session-details">
                            <div class="session-detail">
                                <span class="detail-label">"Role"</span>
                                <span class="detail-value">{agent.role.clone()}</span>
                            </div>
                            <div class="session-detail">
                                <span class="detail-label">"ID"</span>
                                <span class="detail-value"><code>{agent.id.clone()}</code></span>
                            </div>
                        </div>
                        <div class="session-actions">
                            <button
                                class="action-btn"
                                on:click={
                                    let agent_clone = agent.clone();
                                    move |_| set_selected_agent.set(Some(agent_clone.clone()))
                                }
                            >"View Output"</button>
                        </div>
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>

        {move || (!loading.get() && agents.get().is_empty() && error_msg.get().is_none()).then(|| view! {
            <div class="agent-card" style="margin: 24px auto; max-width: 480px; text-align: center; padding: 32px;">
                <h3 style="margin-bottom: 8px;">"No Claude Code Agents"</h3>
                <p style="color: #8b949e;">
                    "No agents with the claude_code role are currently registered. Launch a Claude Code agent to see it here."
                </p>
            </div>
        })}

        // Agent output modal
        {move || selected_agent.get().map(|agent| {
            let agent_name = agent.name.clone();
            let agent_id = agent.id.clone();
            let agent_role = agent.role.clone();
            let agent_status = agent.status.clone();
            let glyph = match agent_status.as_str() {
                "active" | "running" => "glyph-active",
                "idle" => "glyph-idle",
                _ => "glyph-stopped",
            };
            view! {
                <div class="modal-overlay" on:click=move |_| set_selected_agent.set(None)>
                    <div class="modal-content" style="max-width: 520px;" on:click=move |ev| ev.stop_propagation()>
                        <div class="modal-header" style="display: flex; justify-content: space-between; align-items: center;">
                            <h3 style="margin: 0;">"Agent Output"</h3>
                            <button
                                class="btn btn-xs"
                                on:click=move |_| set_selected_agent.set(None)
                                style="background: none; border: none; font-size: 18px; cursor: pointer; color: #8b949e;"
                            >"\u{2715}"</button>
                        </div>
                        <div style="padding: 16px 0;">
                            <div style="margin-bottom: 12px;">
                                <span style="color: #8b949e; font-size: 12px;">"Name"</span>
                                <div style="font-weight: 600; font-size: 16px;">{agent_name}</div>
                            </div>
                            <div style="margin-bottom: 12px;">
                                <span style="color: #8b949e; font-size: 12px;">"ID"</span>
                                <div><code style="font-size: 13px;">{agent_id}</code></div>
                            </div>
                            <div style="margin-bottom: 12px;">
                                <span style="color: #8b949e; font-size: 12px;">"Role"</span>
                                <div>{agent_role}</div>
                            </div>
                            <div style="margin-bottom: 12px;">
                                <span style="color: #8b949e; font-size: 12px;">"Status"</span>
                                <div><span class={glyph}>{agent_status}</span></div>
                            </div>
                            <p style="color: #8b949e; font-size: 13px; margin-top: 16px;">
                                "Terminal output is available on the Terminals page."
                            </p>
                        </div>
                    </div>
                </div>
            }
        })}
    }
}
