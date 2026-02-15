use leptos::prelude::*;

const TABS: &[&str] = &[
    "Dashboard",
    "Agents",
    "Beads",
    "Sessions",
    "Convoys",
    "Costs",
    "Analytics",
    "Config",
    "MCP",
];

#[component]
pub fn NavBar(
    current_tab: ReadSignal<usize>,
    set_current_tab: WriteSignal<usize>,
) -> impl IntoView {
    view! {
        <nav class="nav-bar">
            {TABS.iter().enumerate().map(|(i, label)| {
                let label = *label;
                view! {
                    <button
                        class:nav-tab=true
                        class:active=move || current_tab.get() == i
                        on:click=move |_| set_current_tab.set(i)
                    >
                        {label}
                    </button>
                }
            }).collect::<Vec<_>>()}
        </nav>
    }
}
