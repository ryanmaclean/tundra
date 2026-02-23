use crate::duckdb;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};

use crate::api;
use crate::i18n::t;

#[component]
pub fn AnalyticsPage() -> impl IntoView {
    let (kpi, set_kpi) = signal(Option::<api::ApiKpi>::None);
    let (agents, set_agents) = signal(Vec::<api::ApiAgent>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);

    let do_refresh = move || {
        // Initialize DuckDB WASM in the background
        spawn_local(async move {
            duckdb::init_duckdb().await;
        });

        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::fetch_kpi().await {
                Ok(data) => set_kpi.set(Some(data)),
                Err(e) => set_error_msg.set(Some(format!("Failed to fetch KPI: {e}"))),
            }
            match api::fetch_agents().await {
                Ok(data) => set_agents.set(data),
                Err(_) => {} // non-critical
            }
            set_loading.set(false);
        });
    };

    do_refresh();

    let total_beads = move || kpi.get().as_ref().map(|k| k.total_beads).unwrap_or(0);
    let done_count = move || kpi.get().as_ref().map(|k| k.done).unwrap_or(0);
    let active_agents = move || kpi.get().as_ref().map(|k| k.active_agents).unwrap_or(0);
    let hooked = move || kpi.get().as_ref().map(|k| k.hooked).unwrap_or(0);
    let backlog = move || kpi.get().as_ref().map(|k| k.backlog).unwrap_or(0);
    let failed = move || kpi.get().as_ref().map(|k| k.failed).unwrap_or(0);
    let review = move || kpi.get().as_ref().map(|k| k.review).unwrap_or(0);

    let completion_pct = move || {
        let t = total_beads();
        if t > 0 {
            (done_count() as f64 / t as f64 * 100.0) as u64
        } else {
            0
        }
    };
    let utilization_pct = move || {
        let total_agents = agents.get().len() as u64;
        if total_agents > 0 {
            (active_agents() as f64 / total_agents as f64 * 100.0) as u64
        } else {
            0
        }
    };

    view! {
        <div class="page-header">
            <h2>{t("analytics-title")}</h2>
            <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                {format!("\u{21BB} {}", t("btn-refresh"))}
            </button>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="state-banner state-banner-error">
                <span
                    class="state-banner-icon"
                    inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>"#
                ></span>
                <span>{msg}</span>
            </div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">{t("status-loading")}</div>
        })}

        <div class="kpi-grid">
            <div class="kpi-card">
                <div class="value">{move || total_beads()}</div>
                <div class="label">"Total Beads"</div>
            </div>
            <div class="kpi-card">
                <div class="value">{done_count}</div>
                <div class="label">"Completed"</div>
            </div>
            <div class="kpi-card">
                <div class="value">{active_agents}</div>
                <div class="label">"Active Agents"</div>
            </div>
            <div class="kpi-card">
                <div class="value">{move || format!("{}%", completion_pct())}</div>
                <div class="label">"Completion Rate"</div>
            </div>
        </div>

        <div class="section">
            <h3>"Bead Status Breakdown"</h3>
            <div style="margin-top: 12px;">
                // Bar chart using CSS
                {move || {
                    let items = vec![
                        ("Backlog", backlog(), "#8b949e"),
                        ("In Progress", hooked(), "#1f6feb"),
                        ("Review", review(), "#a371f7"),
                        ("Done", done_count(), "#238636"),
                        ("Failed", failed(), "#da3633"),
                    ];
                    let max_val = items.iter().map(|(_, v, _)| *v).max().unwrap_or(1).max(1);
                    items.into_iter().map(|(label, val, color)| {
                        let pct = (val as f64 / max_val as f64 * 100.0) as u64;
                        view! {
                            <div style="display: flex; align-items: center; margin-bottom: 8px;">
                                <span style="width: 100px; font-size: 0.85em;">{label}</span>
                                <div style="flex: 1; background: #21262d; border-radius: 4px; height: 24px; overflow: hidden;">
                                    <div style={format!(
                                        "width: {}%; background: {}; height: 100%; border-radius: 4px; transition: width 0.3s; min-width: 2px;",
                                        pct, color
                                    )}></div>
                                </div>
                                <span style="width: 50px; text-align: right; font-size: 0.85em;">{format!("{}", val)}</span>
                            </div>
                        }
                    }).collect::<Vec<_>>()
                }}
            </div>
        </div>

        <div class="section">
            <h3>"Agent Utilization"</h3>
            <div style="display: flex; align-items: center; margin-top: 12px;">
                <span style="width: 120px; font-size: 0.85em;">"Utilization"</span>
                <div style="flex: 1; background: #21262d; border-radius: 4px; height: 28px; overflow: hidden;">
                    <div style={move || format!(
                        "width: {}%; background: #1f6feb; height: 100%; border-radius: 4px; transition: width 0.3s; min-width: 2px;",
                        utilization_pct()
                    )}></div>
                </div>
                <span style="width: 60px; text-align: right; font-size: 0.85em;">
                    {move || format!("{}%", utilization_pct())}
                </span>
            </div>
            <div style="margin-top: 12px; color: #8b949e; font-size: 0.85em;">
                {move || format!("{} active out of {} total agents", active_agents(), agents.get().len())}
            </div>
        </div>
    }
}
