use leptos::prelude::*;

const NAV_ITEMS: &[(&str, &str)] = &[
    ("\u{1F3E0}", "Dashboard"),
    ("\u{1F4CB}", "Kanban Board"),
    ("\u{1F5A5}\u{FE0F}", "Agent Terminals"),
    ("\u{1F4CA}", "Insights"),
    ("\u{1F4A1}", "Ideation"),
    ("\u{1F5FA}\u{FE0F}", "Roadmap"),
    ("\u{1F4C1}", "Context"),
    ("\u{1F50C}", "MCP Overview"),
    ("\u{1F333}", "Worktrees"),
    ("\u{1F41B}", "GitHub Issues"),
    ("\u{1F500}", "GitHub PRs"),
    ("\u{1F916}", "Claude Code"),
    ("\u{2699}\u{FE0F}", "Settings"),
];

#[component]
pub fn NavBar(
    current_tab: ReadSignal<usize>,
    set_current_tab: WriteSignal<usize>,
    on_new_task: WriteSignal<bool>,
) -> impl IntoView {
    view! {
        <aside class="sidebar">
            <div class="sidebar-header">
                <div class="sidebar-brand">
                    <div class="sidebar-brand-icon">"AT"</div>
                    <div>
                        <div class="sidebar-brand-name">"auto-tundra"</div>
                        <div class="sidebar-brand-badge">"v0.1.0"</div>
                    </div>
                </div>
            </div>

            <nav class="sidebar-nav">
                <div class="sidebar-section-label">"Main"</div>
                {NAV_ITEMS[..4].iter().enumerate().map(|(i, (icon, label))| {
                    let icon = *icon;
                    let label = *label;
                    view! {
                        <button
                            class="sidebar-item"
                            class:active=move || current_tab.get() == i
                            on:click=move |_| set_current_tab.set(i)
                        >
                            <span class="sidebar-item-icon">{icon}</span>
                            <span class="sidebar-item-label">{label}</span>
                        </button>
                    }
                }).collect::<Vec<_>>()}

                <div class="sidebar-section-label">"Planning"</div>
                {NAV_ITEMS[4..7].iter().enumerate().map(|(j, (icon, label))| {
                    let i = j + 4;
                    let icon = *icon;
                    let label = *label;
                    view! {
                        <button
                            class="sidebar-item"
                            class:active=move || current_tab.get() == i
                            on:click=move |_| set_current_tab.set(i)
                        >
                            <span class="sidebar-item-icon">{icon}</span>
                            <span class="sidebar-item-label">{label}</span>
                        </button>
                    }
                }).collect::<Vec<_>>()}

                <div class="sidebar-section-label">"Infrastructure"</div>
                {NAV_ITEMS[7..9].iter().enumerate().map(|(j, (icon, label))| {
                    let i = j + 7;
                    let icon = *icon;
                    let label = *label;
                    view! {
                        <button
                            class="sidebar-item"
                            class:active=move || current_tab.get() == i
                            on:click=move |_| set_current_tab.set(i)
                        >
                            <span class="sidebar-item-icon">{icon}</span>
                            <span class="sidebar-item-label">{label}</span>
                        </button>
                    }
                }).collect::<Vec<_>>()}

                <div class="sidebar-section-label">"Integrations"</div>
                {NAV_ITEMS[9..12].iter().enumerate().map(|(j, (icon, label))| {
                    let i = j + 9;
                    let icon = *icon;
                    let label = *label;
                    view! {
                        <button
                            class="sidebar-item"
                            class:active=move || current_tab.get() == i
                            on:click=move |_| set_current_tab.set(i)
                        >
                            <span class="sidebar-item-icon">{icon}</span>
                            <span class="sidebar-item-label">{label}</span>
                        </button>
                    }
                }).collect::<Vec<_>>()}

                <div class="sidebar-section-label">"System"</div>
                {NAV_ITEMS[12..].iter().enumerate().map(|(j, (icon, label))| {
                    let i = j + 12;
                    let icon = *icon;
                    let label = *label;
                    view! {
                        <button
                            class="sidebar-item"
                            class:active=move || current_tab.get() == i
                            on:click=move |_| set_current_tab.set(i)
                        >
                            <span class="sidebar-item-icon">{icon}</span>
                            <span class="sidebar-item-label">{label}</span>
                        </button>
                    }
                }).collect::<Vec<_>>()}
            </nav>

            <div class="sidebar-footer">
                <button
                    class="new-task-btn"
                    on:click=move |_| on_new_task.set(true)
                >
                    "+ New Task"
                </button>
            </div>
        </aside>
    }
}

/// Returns the label for a given tab index.
pub fn tab_label(idx: usize) -> &'static str {
    NAV_ITEMS.get(idx).map(|(_, label)| *label).unwrap_or("Kanban Board")
}
