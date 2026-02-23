use crate::state::use_app_state;
use crate::themed::{themed, Prompt};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;

#[component]
pub fn ConvoysPage() -> impl IntoView {
    let app_state = use_app_state();
    let display_mode = app_state.display_mode;
    let (convoys, set_convoys) = signal(Vec::<api::ApiConvoy>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (selected_id, set_selected_id) = signal(Option::<String>::None);

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::fetch_convoys().await {
                Ok(data) => set_convoys.set(data),
                Err(e) => {
                    set_error_msg.set(Some(format!("Failed to fetch convoys: {e}")));
                }
            }
            set_loading.set(false);
        });
    };

    do_refresh();

    let convoy_count = move || convoys.get().len();

    view! {
        <div class="page-header" style="border-bottom: none; flex-wrap: wrap; gap: 8px;">
            <div>
                <h2 style="display: flex; align-items: center; gap: 8px;">
                    "Convoys"
                    <span class="worktree-count-badge">{move || format!("{} Total", convoy_count())}</span>
                </h2>
                <span class="worktree-header-desc">"Coordinate multiple agents working on related tasks"</span>
            </div>
            <div class="page-header-actions" style="margin-left: auto;">
                <button class="action-btn action-forward" disabled=true title="Coming soon">
                    "+ Create Convoy"
                </button>
                <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                    "\u{21BB} Refresh"
                </button>
            </div>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error" style="margin: 0 16px 8px;">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading" style="padding: 0 16px;">{move || themed(display_mode.get(), Prompt::Loading)}</div>
        })}

        // Convoy cards grid
        <div class="worktree-cards">
            {move || {
                let current_selected = selected_id.get();
                convoys.get().into_iter().map(|c| {
                    let cid = c.id.clone();
                    let cid_click = cid.clone();
                    let is_expanded = current_selected.as_deref() == Some(&cid);

                    let status_class = match c.status.to_lowercase().as_str() {
                        "active" => "glyph-active",
                        "forming" => "glyph-pending",
                        "completed" => "glyph-idle",
                        "aborted" => "glyph-stopped",
                        _ => "glyph-unknown",
                    };
                    let status_label = c.status.clone();

                    let bead_count = if c.bead_ids.is_empty() {
                        c.bead_count
                    } else {
                        c.bead_ids.len() as u32
                    };

                    let created_display = c.created_at.clone().unwrap_or_default();
                    let created_short = if created_display.len() >= 10 {
                        created_display[..10].to_string()
                    } else {
                        created_display.clone()
                    };

                    let bead_ids = c.bead_ids.clone();

                    view! {
                        <div
                            class="worktree-card"
                            style="cursor: pointer;"
                            on:click=move |_| {
                                let current = selected_id.get();
                                if current.as_deref() == Some(cid_click.as_str()) {
                                    set_selected_id.set(None);
                                } else {
                                    set_selected_id.set(Some(cid_click.clone()));
                                }
                            }
                        >
                            <div class="worktree-card-top">
                                <div class="worktree-branch-row">
                                    <span class={status_class} style="font-size: 10px;">"\u{25CF} "</span>
                                    <span class="worktree-branch-name">{c.name}</span>
                                </div>
                                <span class="worktree-bead-link">{status_label}</span>
                            </div>
                            <div class="worktree-stats">
                                <span class="worktree-stat-item">
                                    {format!("{} beads", bead_count)}
                                </span>
                                {(!created_short.is_empty()).then(|| view! {
                                    <span>"\u{2022}"</span>
                                    <span class="worktree-stat-item">
                                        {format!("Created {}", created_short)}
                                    </span>
                                })}
                                <span>"\u{2022}"</span>
                                <span class="worktree-stat-item" style="font-family: monospace; font-size: 0.8em;">
                                    {format!("ID: {}...", &c.id.get(..8).unwrap_or(&c.id))}
                                </span>
                            </div>

                            // Expandable detail panel
                            {is_expanded.then(|| {
                                let ids = bead_ids.clone();
                                view! {
                                    <div style="margin-top: 12px; padding-top: 12px; border-top: 1px solid var(--border-color, #30363d);">
                                        <div style="font-size: 0.85em; color: #8b949e; margin-bottom: 6px; font-weight: 600;">
                                            "Bead IDs"
                                        </div>
                                        {if ids.is_empty() {
                                            view! {
                                                <div style="font-size: 0.8em; color: #6e7681; font-style: italic;">
                                                    "No beads assigned"
                                                </div>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <div style="display: flex; flex-wrap: wrap; gap: 4px;">
                                                    {ids.into_iter().map(|bid| {
                                                        let short = bid.get(..8).unwrap_or(&bid).to_string();
                                                        view! {
                                                            <span style="
                                                                font-family: monospace;
                                                                font-size: 0.78em;
                                                                background: var(--card-bg, #161b22);
                                                                padding: 2px 8px;
                                                                border-radius: 4px;
                                                                border: 1px solid var(--border-color, #30363d);
                                                                color: #8b949e;
                                                            ">{short}</span>
                                                        }
                                                    }).collect::<Vec<_>>()}
                                                </div>
                                            }.into_any()
                                        }}
                                    </div>
                                }
                            })}
                        </div>
                    }
                }).collect::<Vec<_>>()
            }}
        </div>

        // Empty state
        {move || (!loading.get() && convoys.get().is_empty() && error_msg.get().is_none()).then(|| view! {
            <div class="worktree-empty">
                <div class="worktree-empty-icon">"--"</div>
                <div class="worktree-empty-text">{move || themed(display_mode.get(), Prompt::EmptyKpi)}</div>
                <div class="worktree-empty-hint">"Convoys coordinate multiple agents working on related tasks with dependency tracking."</div>
            </div>
        })}
    }
}
