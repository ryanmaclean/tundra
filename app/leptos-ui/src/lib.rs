// Leptos view! macro generates closures that rustc flags as "unnecessary
// parentheses" even though the macro requires them for attribute parsing.
#![allow(unused_parens)]

use leptos::prelude::*;

pub mod api;
pub mod components;
pub mod events;
pub mod i18n;
pub mod pages;
pub mod state;
pub mod themed;
pub mod types;

pub use themed::{themed, Prompt};

use wasm_bindgen::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    state::provide_app_state();
    i18n::provide_i18n();

    // Apply display mode + reduced-motion to <body>
    let app_state = state::use_app_state();
    let mode = app_state.display_mode;
    let reduce = app_state.reduce_motion;
    Effect::new(move |_| {
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            if let Some(body) = document.body() {
                let _ = body.set_attribute("data-mode", mode.get().as_str());
                if reduce.get() {
                    let _ = body.class_list().add_1("reduce-motion");
                } else {
                    let _ = body.class_list().remove_1("reduce-motion");
                }
            }
        }
    });

    let (current_tab, set_current_tab) = signal(0usize);
    // Provide set_current_tab as context so child pages can navigate between tabs
    provide_context(set_current_tab);
    let (show_help, set_show_help) = signal(false);
    let (show_new_task, set_show_new_task) = signal(false);
    let (show_settings, set_show_settings) = signal(false);

    // Start the event WebSocket stream
    let (_conn_state, _latest_event, toasts, set_toasts, unread_count, set_unread_count) =
        events::use_event_stream();

    // Fetch notification count on startup so the bell shows the real unread count.
    {
        let set_unread = set_unread_count;
        leptos::task::spawn_local(async move {
            if let Ok(count) = api::fetch_notification_count().await {
                set_unread.set(count.unread);
            }
        });
    }

    let page_label = move || components::nav_bar::tab_label(current_tab.get());

    let project_name = app_state.project_name;

    // Global keyboard shortcuts: pressing a letter key (D, K, A, N, etc.)
    // navigates to the corresponding tab, matching the sidebar shortcut badges.
    {
        let set_tab = set_current_tab;
        Effect::new(move |_| {
            use wasm_bindgen::closure::Closure;
            if let Some(window) = web_sys::window() {
                let handler = Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(
                    move |ev: web_sys::KeyboardEvent| {
                        // Don't trigger shortcuts when typing in an input/textarea/select
                        if let Some(target) = ev.target() {
                            if let Ok(el) = target.dyn_into::<web_sys::HtmlElement>() {
                                let tag = el.tag_name().to_uppercase();
                                if tag == "INPUT" || tag == "TEXTAREA" || tag == "SELECT" {
                                    return;
                                }
                                // Also skip if contenteditable
                                if el.is_content_editable() {
                                    return;
                                }
                            }
                        }
                        // Skip if any modifier key is held (Ctrl, Alt, Meta, Shift)
                        if ev.ctrl_key() || ev.alt_key() || ev.meta_key() || ev.shift_key() {
                            return;
                        }
                        let key = ev.key();
                        // Agents page has local command-bar shortcuts:
                        // H=History, I=Invoke, N=New Terminal, F=Files.
                        if current_tab.get_untracked() == 2
                            && matches!(key.as_str(), "h" | "H" | "i" | "I" | "n" | "N" | "f" | "F")
                        {
                            return;
                        }
                        let tab_idx = match key.as_str() {
                            "d" | "D" => Some(0),  // Dashboard
                            "k" | "K" => Some(1),  // Kanban Board
                            "a" | "A" => Some(2),  // Agent Terminals
                            "n" | "N" => Some(3),  // Insights
                            "i" | "I" => Some(4),  // Ideation
                            "r" | "R" => Some(5),  // Roadmap
                            "l" | "L" => Some(6),  // Changelog
                            "c" | "C" => Some(7),  // Context
                            "m" | "M" => Some(8),  // MCP Overview
                            "w" | "W" => Some(9),  // Worktrees
                            "g" | "G" => Some(10), // GitHub Issues
                            "p" | "P" => Some(11), // GitHub PRs
                            "t" | "T" => Some(14), // Terminals
                            "s" | "S" => Some(16), // Stacks
                            _ => None,
                        };
                        if let Some(idx) = tab_idx {
                            set_tab.set(idx);
                            ev.prevent_default();
                        }
                    },
                );
                let _ = window
                    .add_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref());
                // Leak the closure so it lives for the lifetime of the app
                handler.forget();
            }
        });
    }

    view! {
        <components::project_tabs::ProjectTabs />

        <div class="app-layout">
            // Animated particle background
            <div class="particle-bg">
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
                <div class="particle"></div>
            </div>

            <components::nav_bar::NavBar
                current_tab=current_tab
                set_current_tab=set_current_tab
                on_new_task=set_show_new_task
            />

            <div class="main-area" role="main" style="position: relative; z-index: 2;">
                <header class="top-bar">
                    <div class="top-bar-left">
                        <span class="top-bar-project">{move || project_name.get()}</span>
                        <span class="top-bar-separator" aria-hidden="true">"/"</span>
                        <span class="top-bar-page">{page_label}</span>
                    </div>
                    <div class="top-bar-right">
                        <span class="assigned-tasks-badge" title="Assigned Tasks">
                            "Assigned Tasks "
                            <span class="assigned-tasks-count">{move || app_state.beads.get().len()}</span>
                        </span>
                        <components::notification_bell::NotificationBell
                            unread_count=unread_count
                            set_unread_count=set_unread_count
                            toasts=toasts
                            set_toasts=set_toasts
                        />
                        <button class="refresh-btn topbar-refresh-btn" aria-label="Refresh" on:click=move |_| {
                            let set_beads = app_state.set_beads;
                            let set_agents = app_state.set_agents;
                            let set_status = app_state.set_status;
                            let set_is_demo = app_state.set_is_demo;

                            leptos::task::spawn_local(async move {
                                if let Ok(api_beads) = api::fetch_beads().await {
                                    let real: Vec<crate::types::BeadResponse> = api_beads
                                        .iter()
                                        .map(pages::beads::api_bead_to_bead_response)
                                        .collect();
                                    if !real.is_empty() {
                                        set_is_demo.set(false);

                                        set_beads.set(real);
                                    }
                                }
                                if let Ok(api_agents) = api::fetch_agents().await {
                                    let agents: Vec<crate::types::AgentResponse> = api_agents
                                        .iter()
                                        .map(|a| {
                                            let status = match a.status.as_str() {
                                                "active" => crate::types::AgentStatus::Active,
                                                "idle" => crate::types::AgentStatus::Idle,
                                                "pending" => crate::types::AgentStatus::Pending,
                                                "stopped" => crate::types::AgentStatus::Stopped,
                                                _ => crate::types::AgentStatus::Unknown,
                                            };
                                            crate::types::AgentResponse {
                                                id: a.id.clone(),
                                                name: a.name.clone(),
                                                role: a.role.clone(),
                                                model: String::new(),
                                                status,
                                                tokens_used: 0,
                                                cost_usd: 0.0,
                                            }
                                        })
                                        .collect();
                                    set_agents.set(agents);
                                }
                                if let Ok(st) = api::fetch_status().await {
                                    set_status.set(crate::types::StatusResponse {
                                        daemon_running: true,
                                        active_agents: st.agent_count as u32,
                                        total_beads: st.bead_count as u32,
                                        uptime_secs: st.uptime_secs,
                                    });
                                }
                            });
                        }>
                            "\u{21BB} Refresh"
                        </button>
                        <button
                            class="help-btn topbar-help-btn"
                            aria-label="Show help"
                            on:click=move |_| set_show_help.set(true)
                        >
                            "?"
                        </button>
                    </div>
                </header>

                <div class="main-content">
                    {move || match current_tab.get() {
                        0 => view! { <pages::dashboard::DashboardPage /> }.into_any(),
                        1 => view! { <pages::beads::BeadsPage /> }.into_any(),
                        2 => view! { <pages::agents::AgentsPage /> }.into_any(),
                        3 => view! { <pages::insights::InsightsPage /> }.into_any(),
                        4 => view! { <pages::ideation::IdeationPage /> }.into_any(),
                        5 => view! { <pages::roadmap::RoadmapPage /> }.into_any(),
                        6 => view! { <pages::changelog::ChangelogPage /> }.into_any(),
                        7 => view! { <pages::context::ContextPage /> }.into_any(),
                        8 => view! { <pages::mcp::McpPage /> }.into_any(),
                        9 => view! { <pages::worktrees::WorktreesPage /> }.into_any(),
                        10 => view! { <pages::github_issues::GithubIssuesPage /> }.into_any(),
                        11 => view! { <pages::github_prs::GithubPrsPage /> }.into_any(),
                        12 => view! { <pages::claude_code::ClaudeCodePage /> }.into_any(),
                        13 => {
                            // Settings is now a modal overlay; open it and show Dashboard
                            set_show_settings.set(true);
                            view! { <pages::dashboard::DashboardPage /> }.into_any()
                        },
                        14 => view! { <pages::terminals::TerminalsPage /> }.into_any(),
                        15 => view! { <pages::onboarding::OnboardingPage /> }.into_any(),
                        16 => view! { <pages::stacks::StacksPage /> }.into_any(),
                        _ => view! { <pages::dashboard::DashboardPage /> }.into_any(),
                    }}
                </div>

                <components::status_bar::StatusBar on_help=move || set_show_help.set(true) />
            </div>
        </div>

        {move || show_help.get().then(|| view! {
            <components::help_modal::HelpModal on_close=move |_| set_show_help.set(false) />
        })}

        {move || show_new_task.get().then(|| view! {
            <components::task_wizard::TaskWizard
                on_close=move |_| set_show_new_task.set(false)
            />
        })}

        {move || show_settings.get().then(|| view! {
            <pages::config::ConfigPage
                on_close=Callback::new(move |_| set_show_settings.set(false))
            />
        })}
    }
}

#[wasm_bindgen(start)]
pub fn mount() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
pub mod duckdb;
