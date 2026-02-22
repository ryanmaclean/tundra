use leptos::prelude::*;
use crate::themed::{themed, Prompt};
use leptos::task::spawn_local;

use crate::api;
use crate::i18n::t;
use crate::state::use_app_state;
use crate::types::{AgentStatus, BeadStatus};

#[component]
pub fn DashboardPage() -> impl IntoView {
    let app = use_app_state();
    let display_mode = app.display_mode;

    // Reactive KPI data — derived from demo state, overwritten if backend is up
    let beads = app.beads;
    let agents = app.agents;
    let status = app.status;

    let total_beads = move || beads.get().len() as u64;
    let active_agents = move || agents.get().iter().filter(|a| a.status == AgentStatus::Active).count() as u64;
    let backlog_count = move || beads.get().iter().filter(|b| b.status == BeadStatus::Planning).count() as u64;
    let done_count = move || beads.get().iter().filter(|b| b.status == BeadStatus::Done).count() as u64;
    let hooked_count = move || beads.get().iter().filter(|b| b.status == BeadStatus::InProgress).count() as u64;
    let failed_count = move || beads.get().iter().filter(|b| b.status == BeadStatus::Failed).count() as u64;

    // Status data — seeded from demo, updated if backend available
    let (version, set_version) = signal(String::from("0.1.0"));
    let uptime_secs = move || status.get().uptime_secs;
    let agent_count = move || status.get().active_agents as usize;
    let bead_count = move || status.get().total_beads as usize;

    // Loading / error state
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (_backend_connected, set_backend_connected) = signal(false);

    // Fetch all data — falls back to demo state on failure
    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);

        spawn_local(async move {
            let mut connected = false;

            // Fetch KPI (if backend available)
            if let Ok(_kpi) = api::fetch_kpi().await {
                connected = true;
            }

            // Fetch status (if backend available)
            match api::fetch_status().await {
                Ok(st) => {
                    set_version.set(st.version);
                    connected = true;
                }
                Err(_) => {}
            }

            if !connected {
                set_error_msg.set(Some("Backend not running — showing demo data. Start at-daemon for live data.".to_string()));
            }

            set_backend_connected.set(connected);
            set_loading.set(false);
        });
    };

    // Initial fetch on mount
    do_refresh();

    let format_uptime = move || {
        let secs = uptime_secs();
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let s = secs % 60;
        if hours > 0 {
            format!("{}h {}m {}s", hours, mins, s)
        } else if mins > 0 {
            format!("{}m {}s", mins, s)
        } else {
            format!("{}s", s)
        }
    };

    view! {
        <div class="page-header">
            <h2>{t("dashboard-title")}</h2>
            <button
                class="refresh-btn dashboard-refresh-btn"
                on:click=move |_| do_refresh()
            >
                "\u{21BB} Refresh"
            </button>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error">
                {msg}
            </div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">{move || themed(display_mode.get(), Prompt::Loading)}</div>
        })}

        <div class="dashboard-kpi-grid">
            <div class="dashboard-kpi-card">
                <div class="dashboard-kpi-label">{t("dashboard-beads")}</div>
                <div class="dashboard-kpi-value">{total_beads}</div>
                <div class="dashboard-kpi-subtitle">"All tracked tasks"</div>
            </div>
            <div class="dashboard-kpi-card">
                <div class="dashboard-kpi-label">{t("dashboard-agents")}</div>
                <div class="dashboard-kpi-value">{active_agents}</div>
                <div class="dashboard-kpi-subtitle">"Currently running"</div>
            </div>
            <div class="dashboard-kpi-card">
                <div class="dashboard-kpi-label">"Backlog"</div>
                <div class="dashboard-kpi-value">{backlog_count}</div>
                <div class="dashboard-kpi-subtitle">"Awaiting work"</div>
            </div>
            <div class="dashboard-kpi-card">
                <div class="dashboard-kpi-label">"Done"</div>
                <div class="dashboard-kpi-value">{done_count}</div>
                <div class="dashboard-kpi-subtitle">"Completed tasks"</div>
            </div>
        </div>

        <div class="dashboard-secondary-kpi">
            <div class="dashboard-kpi-card dashboard-kpi-card-sm">
                <div class="dashboard-kpi-label">"In Progress"</div>
                <div class="dashboard-kpi-value">{hooked_count}</div>
            </div>
            <div class="dashboard-kpi-card dashboard-kpi-card-sm">
                <div class="dashboard-kpi-label">"Failed"</div>
                <div class="dashboard-kpi-value dashboard-kpi-value-red">{failed_count}</div>
            </div>
        </div>

        <div class="dashboard-status-section">
            <h3>{t("dashboard-status")}</h3>
            <div class="dashboard-status-grid">
                <div class="dashboard-status-item">
                    <span class="dashboard-status-label">"Version"</span>
                    <span class="dashboard-status-value">{move || version.get()}</span>
                </div>
                <div class="dashboard-status-item">
                    <span class="dashboard-status-label">"Uptime"</span>
                    <span class="dashboard-status-value">{format_uptime}</span>
                </div>
                <div class="dashboard-status-item">
                    <span class="dashboard-status-label">"Agents"</span>
                    <span class="dashboard-status-value">{agent_count}</span>
                </div>
                <div class="dashboard-status-item">
                    <span class="dashboard-status-label">"Beads"</span>
                    <span class="dashboard-status-value">{bead_count}</span>
                </div>
            </div>
        </div>
    }
}
