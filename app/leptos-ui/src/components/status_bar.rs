use leptos::prelude::*;

use crate::state::use_app_state;

#[component]
pub fn StatusBar() -> impl IntoView {
    let state = use_app_state();
    let status = state.status;

    let uptime = move || {
        let secs = status.get().uptime_secs;
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        format!("{:02}:{:02}:{:02}", h, m, s)
    };

    view! {
        <div class="status-bar">
            <div class="left">
                <span>
                    {move || if status.get().daemon_running {
                        view! {
                            <span class="status-dot status-dot-running"></span>
                            "daemon: running"
                        }
                    } else {
                        view! {
                            <span class="status-dot status-dot-stopped"></span>
                            "daemon: stopped"
                        }
                    }}
                </span>
                <span>{move || format!("agents: {}", status.get().active_agents)}</span>
                <span>{move || format!("beads: {}", status.get().total_beads)}</span>
            </div>
            <div class="right">
                <span>{move || format!("uptime: {}", uptime())}</span>
                <span><kbd>"?"</kbd>" help"</span>
            </div>
        </div>
    }
}
