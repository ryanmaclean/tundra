use leptos::prelude::*;

pub mod api;
pub mod types;
pub mod state;
pub mod pages;
pub mod components;

use wasm_bindgen::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    state::provide_app_state();

    let (current_tab, set_current_tab) = signal(0usize);
    let (show_help, set_show_help) = signal(false);
    let (show_new_task, set_show_new_task) = signal(false);

    let page_label = move || {
        components::nav_bar::tab_label(current_tab.get())
    };

    view! {
        <div class="app-layout">
            <components::nav_bar::NavBar
                current_tab=current_tab
                set_current_tab=set_current_tab
                on_new_task=set_show_new_task
            />

            <div class="main-area">
                <div class="top-bar">
                    <div class="top-bar-left">
                        <span class="top-bar-project">"auto-tundra"</span>
                        <span class="top-bar-separator">"/"</span>
                        <span class="top-bar-page">{page_label}</span>
                    </div>
                    <div class="top-bar-right">
                        <button class="refresh-btn">
                            "\u{21BB} Refresh Tasks"
                        </button>
                        <button
                            class="help-btn"
                            on:click=move |_| set_show_help.set(true)
                        >
                            "?"
                        </button>
                    </div>
                </div>

                <div class="main-content">
                    {move || match current_tab.get() {
                        0 => view! { <pages::dashboard::DashboardPage /> }.into_any(),
                        1 => view! { <pages::beads::BeadsPage /> }.into_any(),
                        2 => view! { <pages::agents::AgentsPage /> }.into_any(),
                        3 => view! { <pages::insights::InsightsPage /> }.into_any(),
                        4 => view! { <pages::ideation::IdeationPage /> }.into_any(),
                        5 => view! { <pages::roadmap::RoadmapPage /> }.into_any(),
                        6 => view! { <pages::context::ContextPage /> }.into_any(),
                        7 => view! { <pages::mcp::McpPage /> }.into_any(),
                        8 => view! { <pages::worktrees::WorktreesPage /> }.into_any(),
                        9 => view! { <pages::github_issues::GithubIssuesPage /> }.into_any(),
                        10 => view! { <pages::github_prs::GithubPrsPage /> }.into_any(),
                        11 => view! { <pages::claude_code::ClaudeCodePage /> }.into_any(),
                        12 => view! { <pages::config::ConfigPage /> }.into_any(),
                        _ => view! { <pages::dashboard::DashboardPage /> }.into_any(),
                    }}
                </div>

                <components::status_bar::StatusBar />
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
    }
}

#[wasm_bindgen(start)]
pub fn mount() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
