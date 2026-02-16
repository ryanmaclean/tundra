use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;

#[component]
pub fn DashboardPage() -> impl IntoView {
    // Signals for KPI data
    let (total_beads, set_total_beads) = signal(0u64);
    let (active_agents, set_active_agents) = signal(0u64);
    let (backlog_count, set_backlog_count) = signal(0u64);
    let (done_count, set_done_count) = signal(0u64);
    let (hooked_count, set_hooked_count) = signal(0u64);
    let (failed_count, set_failed_count) = signal(0u64);

    // Signals for status data
    let (version, set_version) = signal(String::from("--"));
    let (uptime_secs, set_uptime_secs) = signal(0u64);
    let (agent_count, set_agent_count) = signal(0usize);
    let (bead_count, set_bead_count) = signal(0usize);

    // Loading / error state
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);

    // Fetch all data
    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);

        spawn_local(async move {
            // Fetch KPI
            match api::fetch_kpi().await {
                Ok(kpi) => {
                    set_total_beads.set(kpi.total_beads);
                    set_active_agents.set(kpi.active_agents);
                    set_backlog_count.set(kpi.backlog);
                    set_done_count.set(kpi.done);
                    set_hooked_count.set(kpi.hooked);
                    set_failed_count.set(kpi.failed);
                }
                Err(e) => {
                    set_error_msg.set(Some(format!("KPI fetch failed: {e}")));
                }
            }

            // Fetch status
            match api::fetch_status().await {
                Ok(st) => {
                    set_version.set(st.version);
                    set_uptime_secs.set(st.uptime_secs);
                    set_agent_count.set(st.agent_count);
                    set_bead_count.set(st.bead_count);
                }
                Err(e) => {
                    let prev = error_msg.get_untracked().unwrap_or_default();
                    let msg = if prev.is_empty() {
                        format!("Status fetch failed: {e}")
                    } else {
                        format!("{prev} | Status fetch failed: {e}")
                    };
                    set_error_msg.set(Some(msg));
                }
            }

            set_loading.set(false);
        });
    };

    // Initial fetch on mount
    do_refresh();

    let format_uptime = move || {
        let secs = uptime_secs.get();
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
            <h2>"Dashboard"</h2>
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
            <div class="dashboard-loading">"Loading data from backend..."</div>
        })}

        <div class="dashboard-kpi-grid">
            <div class="dashboard-kpi-card">
                <div class="dashboard-kpi-label">"Total Beads"</div>
                <div class="dashboard-kpi-value">{move || total_beads.get()}</div>
                <div class="dashboard-kpi-subtitle">"All tracked tasks"</div>
            </div>
            <div class="dashboard-kpi-card">
                <div class="dashboard-kpi-label">"Active Agents"</div>
                <div class="dashboard-kpi-value">{move || active_agents.get()}</div>
                <div class="dashboard-kpi-subtitle">"Currently running"</div>
            </div>
            <div class="dashboard-kpi-card">
                <div class="dashboard-kpi-label">"Backlog"</div>
                <div class="dashboard-kpi-value">{move || backlog_count.get()}</div>
                <div class="dashboard-kpi-subtitle">"Awaiting work"</div>
            </div>
            <div class="dashboard-kpi-card">
                <div class="dashboard-kpi-label">"Done"</div>
                <div class="dashboard-kpi-value">{move || done_count.get()}</div>
                <div class="dashboard-kpi-subtitle">"Completed tasks"</div>
            </div>
        </div>

        <div class="dashboard-secondary-kpi">
            <div class="dashboard-kpi-card dashboard-kpi-card-sm">
                <div class="dashboard-kpi-label">"In Progress"</div>
                <div class="dashboard-kpi-value">{move || hooked_count.get()}</div>
            </div>
            <div class="dashboard-kpi-card dashboard-kpi-card-sm">
                <div class="dashboard-kpi-label">"Failed"</div>
                <div class="dashboard-kpi-value dashboard-kpi-value-red">{move || failed_count.get()}</div>
            </div>
        </div>

        <div class="dashboard-status-section">
            <h3>"System Status"</h3>
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
                    <span class="dashboard-status-value">{move || agent_count.get()}</span>
                </div>
                <div class="dashboard-status-item">
                    <span class="dashboard-status-label">"Beads"</span>
                    <span class="dashboard-status-value">{move || bead_count.get()}</span>
                </div>
            </div>
        </div>
    }
}
