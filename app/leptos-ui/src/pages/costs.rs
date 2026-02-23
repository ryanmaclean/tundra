use crate::duckdb;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};

use crate::api;
use crate::i18n::t;

#[component]
pub fn CostsPage() -> impl IntoView {
    let (costs, set_costs) = signal(Option::<api::ApiCosts>::None);
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
            // Try /api/costs first, fall back to /api/kpi
            match api::fetch_costs().await {
                Ok(data) => set_costs.set(Some(data)),
                Err(_) => {
                    // Fallback: build cost data from KPI
                    match api::fetch_kpi().await {
                        Ok(kpi) => {
                            // Estimate tokens from KPI fields
                            let est_input = kpi.total_beads * 5000; // rough estimate
                            let est_output = kpi.total_beads * 2000;
                            set_costs.set(Some(api::ApiCosts {
                                input_tokens: est_input,
                                output_tokens: est_output,
                                sessions: vec![],
                            }));
                        }
                        Err(e) => {
                            set_error_msg.set(Some(format!("Failed to fetch cost data: {e}")))
                        }
                    }
                }
            }
            set_loading.set(false);
        });
    };

    do_refresh();

    let input_cost_per_m = 3.0_f64;
    let output_cost_per_m = 15.0_f64;

    let total_input = move || costs.get().as_ref().map(|c| c.input_tokens).unwrap_or(0);
    let total_output = move || costs.get().as_ref().map(|c| c.output_tokens).unwrap_or(0);
    let total_cost = move || {
        let input = total_input() as f64 / 1_000_000.0 * input_cost_per_m;
        let output = total_output() as f64 / 1_000_000.0 * output_cost_per_m;
        input + output
    };
    let input_cost = move || total_input() as f64 / 1_000_000.0 * input_cost_per_m;
    let output_cost = move || total_output() as f64 / 1_000_000.0 * output_cost_per_m;

    view! {
        <div class="page-header">
            <h2>{t("costs-breakdown")}</h2>
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

        <div class="kpi-grid" style="grid-template-columns: repeat(3, 1fr);">
            <div class="kpi-card">
                <div class="value">{move || format!("${:.4}", total_cost())}</div>
                <div class="label">{t("costs-total")}</div>
            </div>
            <div class="kpi-card">
                <div class="value">{move || format!("{}", total_input())}</div>
                <div class="label">"Input Tokens"</div>
            </div>
            <div class="kpi-card">
                <div class="value">{move || format!("{}", total_output())}</div>
                <div class="label">"Output Tokens"</div>
            </div>
        </div>

        <div class="section">
            <h3>"Cost Distribution"</h3>
            <p style="color: #8b949e; font-size: 0.85em; margin-bottom: 12px;">
                "Rates: $3.00/1M input tokens, $15.00/1M output tokens (Claude)"
            </p>
            <div style="margin-bottom: 16px;">
                <div style="display: flex; align-items: center; margin-bottom: 8px;">
                    <span style="width: 120px; font-size: 0.85em;">"Input Cost"</span>
                    <div style="flex: 1; background: #21262d; border-radius: 4px; height: 24px; overflow: hidden;">
                        <div style={move || {
                            let tc = total_cost();
                            let pct = if tc > 0.0 { input_cost() / tc * 100.0 } else { 0.0 };
                            format!("width: {:.0}%; background: #238636; height: 100%; border-radius: 4px; min-width: 2px;", pct)
                        }}></div>
                    </div>
                    <span style="width: 80px; text-align: right; font-size: 0.85em;">
                        {move || format!("${:.4}", input_cost())}
                    </span>
                </div>
                <div style="display: flex; align-items: center;">
                    <span style="width: 120px; font-size: 0.85em;">"Output Cost"</span>
                    <div style="flex: 1; background: #21262d; border-radius: 4px; height: 24px; overflow: hidden;">
                        <div style={move || {
                            let tc = total_cost();
                            let pct = if tc > 0.0 { output_cost() / tc * 100.0 } else { 0.0 };
                            format!("width: {:.0}%; background: #da3633; height: 100%; border-radius: 4px; min-width: 2px;", pct)
                        }}></div>
                    </div>
                    <span style="width: 80px; text-align: right; font-size: 0.85em;">
                        {move || format!("${:.4}", output_cost())}
                    </span>
                </div>
            </div>
        </div>

        // Per-session breakdown
        {move || {
            let sessions = costs.get().map(|c| c.sessions).unwrap_or_default();
            if sessions.is_empty() {
                return Vec::new();
            }
            vec![view! {
                <div class="section">
                    <h3>"Per-Session Breakdown"</h3>
                    <table class="data-table">
                        <thead>
                            <tr>
                                <th>"Session"</th>
                                <th>"Agent"</th>
                                <th>"Input Tokens"</th>
                                <th>"Output Tokens"</th>
                                <th>"Est. Cost"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {sessions.into_iter().map(|s| {
                                let cost = s.input_tokens as f64 / 1_000_000.0 * input_cost_per_m
                                    + s.output_tokens as f64 / 1_000_000.0 * output_cost_per_m;
                                view! {
                                    <tr>
                                        <td><code>{s.session_id}</code></td>
                                        <td>{s.agent_name}</td>
                                        <td>{format!("{}", s.input_tokens)}</td>
                                        <td>{format!("{}", s.output_tokens)}</td>
                                        <td>{format!("${:.4}", cost)}</td>
                                    </tr>
                                }
                            }).collect::<Vec<_>>()}
                        </tbody>
                    </table>
                </div>
            }]
        }}
    }
}
