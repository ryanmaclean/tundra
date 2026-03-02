use crate::analytics_store;
use crate::api;
use crate::components::spinner::Spinner;
use crate::i18n::t;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn CostsPage() -> impl IntoView {
    let (costs, set_costs) = signal(Option::<api::ApiCosts>::None);
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);

    // DuckDB-powered breakdowns
    let (by_provider, set_by_provider) = signal(Vec::<analytics_store::ProviderCost>::new());
    let (by_model, set_by_model) = signal(Vec::<analytics_store::ModelCost>::new());
    let (daily_trend, set_daily_trend) = signal(Vec::<analytics_store::DailyCost>::new());

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            // Initialize DuckDB and load cost data
            match analytics_store::init_and_load().await {
                Ok(client) => {
                    let providers = analytics_store::cost_by_provider(&client).await;
                    set_by_provider.set(providers);

                    let models = analytics_store::cost_by_model(&client).await;
                    set_by_model.set(models);

                    let trend = analytics_store::daily_cost_trend(&client).await;
                    set_daily_trend.set(trend);
                }
                Err(e) => {
                    web_sys::console::warn_1(&format!("DuckDB init failed: {e}").into());
                }
            }

            // Also fetch direct costs for the summary cards
            match api::fetch_costs().await {
                Ok(data) => set_costs.set(Some(data)),
                Err(_) => match api::fetch_kpi().await {
                    Ok(kpi) => {
                        let est_input = kpi.total_beads * 5000;
                        let est_output = kpi.total_beads * 2000;
                        set_costs.set(Some(api::ApiCosts {
                            input_tokens: est_input,
                            output_tokens: est_output,
                            sessions: vec![],
                        }));
                    }
                    Err(e) => set_error_msg.set(Some(format!("Failed to fetch cost data: {e}"))),
                },
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
            <Spinner size="md" label=""/>
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

        // DuckDB-powered: Cost by Provider
        {move || {
            let providers = by_provider.get();
            if providers.is_empty() {
                Vec::new()
            } else {
                let max_cost = providers.iter().map(|p| p.total_cost).fold(0.0_f64, f64::max).max(0.01);
                vec![view! {
                    <div class="section">
                        <h3>"Cost by Provider (DuckDB)"</h3>
                        <div style="margin-top: 12px;">
                            {providers.iter().cloned().map(|p| {
                                let pct = (p.total_cost / max_cost * 100.0) as u64;
                                view! {
                                    <div style="display: flex; align-items: center; margin-bottom: 8px;">
                                        <span style="width: 120px; font-size: 0.85em;">{p.provider.clone()}</span>
                                        <div style="flex: 1; background: #21262d; border-radius: 4px; height: 24px; overflow: hidden;">
                                            <div style={format!(
                                                "width: {}%; background: #1f6feb; height: 100%; border-radius: 4px; min-width: 2px;",
                                                pct
                                            )}></div>
                                        </div>
                                        <span style="width: 80px; text-align: right; font-size: 0.85em;">
                                            {format!("${:.4}", p.total_cost)}
                                        </span>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    </div>
                }]
            }
        }}

        // DuckDB-powered: Cost by Model
        {move || {
            let models = by_model.get();
            if models.is_empty() {
                Vec::new()
            } else {
                vec![view! {
                    <div class="section">
                        <h3>"Cost by Model (DuckDB)"</h3>
                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>"Model"</th>
                                    <th>"Total Tokens"</th>
                                    <th>"Total Cost"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {models.iter().cloned().map(|m| {
                                    view! {
                                        <tr>
                                            <td>{m.model}</td>
                                            <td>{format!("{}", m.total_tokens)}</td>
                                            <td>{format!("${:.4}", m.total_cost)}</td>
                                        </tr>
                                    }
                                }).collect::<Vec<_>>()}
                            </tbody>
                        </table>
                    </div>
                }]
            }
        }}

        // DuckDB-powered: Daily Cost Trend (window function)
        {move || {
            let trend = daily_trend.get();
            if trend.is_empty() {
                Vec::new()
            } else {
                vec![view! {
                    <div class="section">
                        <h3>"Daily Cost Trend (DuckDB)"</h3>
                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>"Date"</th>
                                    <th>"Daily Cost"</th>
                                    <th>"Cumulative"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {trend.iter().cloned().map(|d| {
                                    view! {
                                        <tr>
                                            <td>{d.day}</td>
                                            <td>{format!("${:.4}", d.total_cost)}</td>
                                            <td>{format!("${:.4}", d.cumulative_cost)}</td>
                                        </tr>
                                    }
                                }).collect::<Vec<_>>()}
                            </tbody>
                        </table>
                    </div>
                }]
            }
        }}

        // Per-session breakdown (direct from API)
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
