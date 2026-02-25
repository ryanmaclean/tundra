use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::analytics_store;
use crate::api;
use crate::i18n::t;
use crate::webgpu;

#[component]
pub fn AnalyticsPage() -> impl IntoView {
    let (kpi, set_kpi) = signal(Option::<api::ApiKpi>::None);
    let (agents, set_agents) = signal(Vec::<api::ApiAgent>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);

    // DuckDB-powered analytics
    let (phase_counts, set_phase_counts) = signal(Vec::<analytics_store::PhaseCount>::new());
    let (avg_durations, set_avg_durations) = signal(Vec::<analytics_store::AvgDuration>::new());
    let (cost_by_provider, set_cost_by_provider) =
        signal(Vec::<analytics_store::ProviderCost>::new());
    let (webgpu_probe, set_webgpu_probe) = signal(Option::<webgpu::WebGpuProbeReport>::None);
    let (webgpu_running, set_webgpu_running) = signal(false);

    let run_webgpu_probe = move || {
        set_webgpu_running.set(true);
        spawn_local(async move {
            match webgpu::probe(256).await {
                Ok(report) => set_webgpu_probe.set(Some(report)),
                Err(e) => {
                    set_webgpu_probe.set(Some(webgpu::WebGpuProbeReport {
                        supported: false,
                        error: Some(e),
                        ..Default::default()
                    }));
                }
            }
            set_webgpu_running.set(false);
        });
    };

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            // Initialize DuckDB and load data
            match analytics_store::init_and_load().await {
                Ok(client) => {
                    // Run analytical queries
                    let phases = analytics_store::tasks_by_phase(&client).await;
                    set_phase_counts.set(phases);

                    let durations = analytics_store::avg_duration_by_phase(&client).await;
                    set_avg_durations.set(durations);

                    let providers = analytics_store::cost_by_provider(&client).await;
                    set_cost_by_provider.set(providers);
                }
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!("DuckDB init failed: {e}").into(),
                    );
                }
            }

            // Also fetch direct KPI for the summary cards
            match api::fetch_kpi().await {
                Ok(data) => set_kpi.set(Some(data)),
                Err(e) => set_error_msg.set(Some(format!("Failed to fetch KPI: {e}"))),
            }
            match api::fetch_agents().await {
                Ok(data) => set_agents.set(data),
                Err(_) => {}
            }
            set_loading.set(false);
        });
        run_webgpu_probe();
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
            <div style="display:flex;align-items:center;justify-content:space-between;">
                <h3>"WebGPU Probe (M-series)"</h3>
                <button
                    class="refresh-btn dashboard-refresh-btn"
                    on:click=move |_| run_webgpu_probe()
                    disabled=move || webgpu_running.get()
                >
                    {move || if webgpu_running.get() { "Running...".to_string() } else { "Run Probe".to_string() }}
                </button>
            </div>
            {move || {
                match webgpu_probe.get() {
                    None => view! {
                        <p style="color:#8b949e;font-size:0.85em;">"No probe data yet."</p>
                    }.into_any(),
                    Some(report) => {
                        if report.supported {
                            view! {
                                <div style="display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px;margin-top:8px;">
                                    <div class="kpi-chip">
                                        <span class="kpi-chip-label">"Adapter"</span>
                                        <span class="kpi-chip-value">{report.adapter.unwrap_or_else(|| "unknown".to_string())}</span>
                                    </div>
                                    <div class="kpi-chip">
                                        <span class="kpi-chip-label">"Elapsed (ms)"</span>
                                        <span class="kpi-chip-value">
                                            {format!("{:.3}", report.elapsed_ms.unwrap_or(0.0))}
                                        </span>
                                    </div>
                                    <div class="kpi-chip">
                                        <span class="kpi-chip-label">"Architecture"</span>
                                        <span class="kpi-chip-value">{report.architecture.unwrap_or_else(|| "n/a".to_string())}</span>
                                    </div>
                                    <div class="kpi-chip">
                                        <span class="kpi-chip-label">"Sample Output"</span>
                                        <span class="kpi-chip-value">
                                            {format!("{:?}", report.sample)}
                                        </span>
                                    </div>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <p style="color:#f85149;font-size:0.85em;margin-top:8px;">
                                    {report.error.unwrap_or_else(|| "WebGPU unavailable".to_string())}
                                </p>
                            }.into_any()
                        }
                    }
                }
            }}
        </div>

        // DuckDB-powered: Tasks by Phase
        <div class="section">
            <h3>"Tasks by Phase (DuckDB)"</h3>
            <div style="margin-top: 12px;">
                {move || {
                    let phases = phase_counts.get();
                    {
                        // Build a unified list of (label, count, color)
                        let items: Vec<(String, i64, String)> = if phases.is_empty() {
                            vec![
                                ("Backlog".to_string(), backlog() as i64, "#8b949e".to_string()),
                                ("In Progress".to_string(), hooked() as i64, "#1f6feb".to_string()),
                                ("Review".to_string(), review() as i64, "#a371f7".to_string()),
                                ("Done".to_string(), done_count() as i64, "#238636".to_string()),
                                ("Failed".to_string(), failed() as i64, "#da3633".to_string()),
                            ]
                        } else {
                            phases.iter().cloned().map(|p| {
                                let color = match p.phase.to_lowercase().as_str() {
                                    "done" => "#238636",
                                    "failed" => "#da3633",
                                    "backlog" => "#8b949e",
                                    "review" | "ai review" | "human review" => "#a371f7",
                                    _ => "#1f6feb",
                                };
                                (p.phase, p.count, color.to_string())
                            }).collect()
                        };
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
                    }
                }}
            </div>
        </div>

        // DuckDB-powered: Average Duration by Phase
        {move || {
            let durations = avg_durations.get();
            if durations.is_empty() {
                Vec::new()
            } else {
                vec![view! {
                    <div class="section">
                        <h3>"Average Duration by Phase (DuckDB)"</h3>
                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>"Phase"</th>
                                    <th>"Avg Duration"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {durations.iter().cloned().map(|d| {
                                    let formatted = if d.avg_seconds < 60.0 {
                                        format!("{:.0}s", d.avg_seconds)
                                    } else if d.avg_seconds < 3600.0 {
                                        format!("{:.1}m", d.avg_seconds / 60.0)
                                    } else {
                                        format!("{:.1}h", d.avg_seconds / 3600.0)
                                    };
                                    view! {
                                        <tr>
                                            <td>{d.phase}</td>
                                            <td>{formatted}</td>
                                        </tr>
                                    }
                                }).collect::<Vec<_>>()}
                            </tbody>
                        </table>
                    </div>
                }]
            }
        }}

        // DuckDB-powered: Cost by Provider
        {move || {
            let providers = cost_by_provider.get();
            if providers.is_empty() {
                Vec::new()
            } else {
                vec![view! {
                    <div class="section">
                        <h3>"Cost by Provider (DuckDB)"</h3>
                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>"Provider"</th>
                                    <th>"Total Tokens"</th>
                                    <th>"Total Cost"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {providers.iter().cloned().map(|p| {
                                    view! {
                                        <tr>
                                            <td>{p.provider}</td>
                                            <td>{format!("{}", p.total_tokens)}</td>
                                            <td>{format!("${:.4}", p.total_cost)}</td>
                                        </tr>
                                    }
                                }).collect::<Vec<_>>()}
                            </tbody>
                        </table>
                    </div>
                }]
            }
        }}

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
