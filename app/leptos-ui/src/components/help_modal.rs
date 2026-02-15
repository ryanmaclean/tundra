use leptos::prelude::*;
use leptos::ev::MouseEvent;

const HELP_ITEMS: &[(&str, &str)] = &[
    ("Show help", "?"),
    ("Kanban Board", "1"),
    ("Agent Terminals", "2"),
    ("Insights", "3"),
    ("Ideation", "4"),
    ("Roadmap", "5"),
    ("Context", "6"),
    ("MCP Overview", "7"),
    ("Worktrees", "8"),
    ("GitHub Issues", "9"),
    ("GitHub PRs", "0"),
    ("Claude Code", "-"),
    ("Settings", "="),
];

#[component]
pub fn HelpModal(
    on_close: impl Fn(MouseEvent) + 'static,
) -> impl IntoView {
    view! {
        <div class="help-overlay" on:click=on_close>
        </div>
        <div class="help-modal">
            <h2>"Keyboard Shortcuts"</h2>
            {HELP_ITEMS.iter().map(|(label, key)| {
                let label = *label;
                let key = *key;
                view! {
                    <div class="keybind">
                        <span>{label}</span>
                        <kbd>{key}</kbd>
                    </div>
                }
            }).collect::<Vec<_>>()}
            <button class="close-btn">"Close"</button>
        </div>
    }
}
