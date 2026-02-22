use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::i18n::t;
use crate::state::use_app_state;
use crate::themed::{themed, Prompt};
use crate::types::{AgentStatus, BeadStatus};

fn activity_icon_class(status: &BeadStatus) -> &'static str {
    match status {
        BeadStatus::Planning => "dash-activity-icon dash-icon-planning",
        BeadStatus::InProgress => "dash-activity-icon dash-icon-progress",
        BeadStatus::AiReview | BeadStatus::HumanReview => "dash-activity-icon dash-icon-review",
        BeadStatus::Done => "dash-activity-icon dash-icon-done",
        BeadStatus::Failed => "dash-activity-icon dash-icon-failed",
    }
}

fn activity_icon(status: &BeadStatus) -> &'static str {
    match status {
        BeadStatus::Planning => "◇",
        BeadStatus::InProgress => "▶",
        BeadStatus::AiReview | BeadStatus::HumanReview => "◎",
        BeadStatus::Done => "✓",
        BeadStatus::Failed => "⚠",
    }
}

#[component]
pub fn DashboardPage() -> impl IntoView {
    let app = use_app_state();
    let display_mode = app.display_mode;

    let beads = app.beads;
    let agents = app.agents;
    let status = app.status;

    let total_beads = move || beads.get().len() as u64;
    let active_agents = move || {
        agents
            .get()
            .iter()
            .filter(|a| a.status == AgentStatus::Active)
            .count() as u64
    };
    let planning_count = move || {
        beads
            .get()
            .iter()
            .filter(|b| b.status == BeadStatus::Planning)
            .count() as u64
    };
    let progress_count = move || {
        beads
            .get()
            .iter()
            .filter(|b| b.status == BeadStatus::InProgress)
            .count() as u64
    };
    let review_count = move || {
        beads
            .get()
            .iter()
            .filter(|b| matches!(b.status, BeadStatus::AiReview | BeadStatus::HumanReview))
            .count() as u64
    };
    let done_count = move || {
        beads
            .get()
            .iter()
            .filter(|b| b.status == BeadStatus::Done)
            .count() as u64
    };
    let failed_count = move || {
        beads
            .get()
            .iter()
            .filter(|b| b.status == BeadStatus::Failed)
            .count() as u64
    };

    let (version, set_version) = signal(String::from("0.1.0"));
    let uptime_secs = move || status.get().uptime_secs;
    let agent_count = move || status.get().active_agents as usize;
    let bead_count = move || status.get().total_beads as usize;

    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (_backend_connected, set_backend_connected) = signal(false);

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);

        spawn_local(async move {
            let mut connected = false;

            if api::fetch_kpi().await.is_ok() {
                connected = true;
            }

            if let Ok(st) = api::fetch_status().await {
                set_version.set(st.version);
                connected = true;
            }

            if !connected {
                set_error_msg.set(Some(
                    "Backend not running — showing demo data. Start at-daemon for live data."
                        .to_string(),
                ));
            }

            set_backend_connected.set(connected);
            set_loading.set(false);
        });
    };

    do_refresh();

    let format_uptime = move || {
        let secs = uptime_secs();
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let s = secs % 60;
        if hours > 0 {
            format!("{hours}h {mins}m {s}s")
        } else if mins > 0 {
            format!("{mins}m {s}s")
        } else {
            format!("{s}s")
        }
    };

    view! {
        <div class="page-header">
            <h2>{t("dashboard-title")}</h2>
            <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                "\u{21BB} Refresh"
            </button>
        </div>

        <div class="dash-page">
            {move || error_msg.get().map(|msg| view! {
                <div class="dash-error">{msg}</div>
            })}

            {move || loading.get().then(|| view! {
                <div class="dash-loading">{move || themed(display_mode.get(), Prompt::Loading)}</div>
            })}

            <div class="dash-kpi-row">
                <div class="dash-kpi-chip">
                    <span class="dash-kpi-dot dash-dot-purple"></span>
                    <span class="dash-kpi-num">{total_beads}</span>
                    <span class="dash-kpi-label">"BEADS"</span>
                </div>
                <div class="dash-kpi-chip">
                    <span class="dash-kpi-dot dash-dot-green"></span>
                    <span class="dash-kpi-num">{active_agents}</span>
                    <span class="dash-kpi-label">"ACTIVE AGENTS"</span>
                </div>
                <div class="dash-kpi-chip">
                    <span class="dash-kpi-dot dash-dot-amber"></span>
                    <span class="dash-kpi-num">{planning_count}</span>
                    <span class="dash-kpi-label">"PLANNING"</span>
                </div>
                <div class="dash-kpi-chip">
                    <span class="dash-kpi-dot dash-dot-blue"></span>
                    <span class="dash-kpi-num">{progress_count}</span>
                    <span class="dash-kpi-label">"IN PROGRESS"</span>
                </div>
                <div class="dash-kpi-chip">
                    <span class="dash-kpi-dot dash-dot-purple"></span>
                    <span class="dash-kpi-num">{review_count}</span>
                    <span class="dash-kpi-label">"REVIEW"</span>
                </div>
                <div class="dash-kpi-chip">
                    <span class="dash-kpi-dot dash-dot-green"></span>
                    <span class="dash-kpi-num">{done_count}</span>
                    <span class="dash-kpi-label">"DONE"</span>
                </div>
                <div class="dash-kpi-chip">
                    <span class="dash-kpi-dot dash-dot-red"></span>
                    <span class="dash-kpi-num">{failed_count}</span>
                    <span class="dash-kpi-label">"FAILED"</span>
                </div>
            </div>

            <div class="dash-body">
                <div class="dash-panel">
                    <div class="dash-panel-header">
                        <span class="dash-panel-title">"RECENT ACTIVITY"</span>
                        <span class="dash-panel-badge">{move || format!("{} items", beads.get().len())}</span>
                    </div>
                    <div class="dash-activity-list">
                        {move || {
                            let items = beads.get();
                            if items.is_empty() {
                                return vec![view! { <div class="dash-activity-empty">"No activity yet"</div> }.into_any()];
                            }
                            items
                                .into_iter()
                                .take(12)
                                .map(|bead| {
                                    let icon_cls = activity_icon_class(&bead.status);
                                    let icon = activity_icon(&bead.status);
                                    let meta = if let Some(agent) = bead.agent_names.first() {
                                        format!("{agent} · {:?}", bead.status)
                                    } else {
                                        format!("unassigned · {:?}", bead.status)
                                    };
                                    let ts = if bead.timestamp.is_empty() {
                                        "just now".to_string()
                                    } else {
                                        bead.timestamp.clone()
                                    };
                                    view! {
                                        <div class="dash-activity-item">
                                            <span class={icon_cls}>{icon}</span>
                                            <div class="dash-activity-content">
                                                <span class="dash-activity-title">{bead.title.clone()}</span>
                                                <span class="dash-activity-meta">{meta}</span>
                                            </div>
                                            <span class="dash-activity-time">{ts}</span>
                                        </div>
                                    }
                                    .into_any()
                                })
                                .collect::<Vec<_>>()
                        }}
                    </div>
                </div>

                <div class="dash-panel">
                    <div class="dash-panel-header">
                        <span class="dash-panel-title">"AGENT STATUS"</span>
                        <span class="dash-panel-badge">{move || format!("{} total", agents.get().len())}</span>
                    </div>
                    <div class="dash-agent-list">
                        {move || {
                            let list = agents.get();
                            if list.is_empty() {
                                return vec![view! { <div class="dash-activity-empty">"No agents registered"</div> }.into_any()];
                            }
                            list.into_iter()
                                .map(|agent| {
                                    let dot_class = match agent.status {
                                        AgentStatus::Active => "dash-agent-dot dash-dot-active",
                                        AgentStatus::Idle => "dash-agent-dot dash-dot-idle",
                                        AgentStatus::Pending => "dash-agent-dot dash-dot-pending",
                                        AgentStatus::Stopped => "dash-agent-dot dash-dot-stopped",
                                        AgentStatus::Unknown => "dash-agent-dot dash-dot-idle",
                                    };
                                    let status_label = format!("{:?}", agent.status).to_lowercase();
                                    view! {
                                        <div class="dash-agent-item">
                                            <span class={dot_class}></span>
                                            <div class="dash-agent-info">
                                                <div class="dash-agent-name-row">
                                                    <span class="dash-agent-name">{agent.name.clone()}</span>
                                                    <span class="dash-agent-status">{status_label}</span>
                                                </div>
                                                <span class="dash-agent-role">{agent.role.clone()}</span>
                                            </div>
                                        </div>
                                    }
                                    .into_any()
                                })
                                .collect::<Vec<_>>()
                        }}
                    </div>
                </div>
            </div>

            <div class="dash-footer">
                <span class="dash-footer-item"><span class="dash-footer-label">"version"</span>" " {move || version.get()}</span>
                <span class="dash-footer-sep">"|"</span>
                <span class="dash-footer-item"><span class="dash-footer-label">"uptime"</span>" " {format_uptime}</span>
                <span class="dash-footer-sep">"|"</span>
                <span class="dash-footer-item"><span class="dash-footer-label">"agents"</span>" " {agent_count}</span>
                <span class="dash-footer-sep">"|"</span>
                <span class="dash-footer-item"><span class="dash-footer-label">"beads"</span>" " {bead_count}</span>
            </div>
        </div>
    }
}
