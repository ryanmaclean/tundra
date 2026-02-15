use leptos::prelude::*;

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

    view! {
        <components::nav_bar::NavBar current_tab=current_tab set_current_tab=set_current_tab />
        <div class="content">
            {move || match current_tab.get() {
                0 => view! { <pages::dashboard::DashboardPage /> }.into_any(),
                1 => view! { <pages::agents::AgentsPage /> }.into_any(),
                2 => view! { <pages::beads::BeadsPage /> }.into_any(),
                3 => view! { <pages::sessions::SessionsPage /> }.into_any(),
                4 => view! { <pages::convoys::ConvoysPage /> }.into_any(),
                5 => view! { <pages::costs::CostsPage /> }.into_any(),
                6 => view! { <pages::analytics::AnalyticsPage /> }.into_any(),
                7 => view! { <pages::config::ConfigPage /> }.into_any(),
                8 => view! { <pages::mcp::McpPage /> }.into_any(),
                _ => view! { <pages::dashboard::DashboardPage /> }.into_any(),
            }}
        </div>
        <components::status_bar::StatusBar />
        {move || show_help.get().then(|| view! {
            <components::help_modal::HelpModal on_close=move |_| set_show_help.set(false) />
        })}
    }
}

#[wasm_bindgen(start)]
pub fn mount() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
