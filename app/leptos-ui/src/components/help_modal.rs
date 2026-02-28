use leptos::ev::{KeyboardEvent, MouseEvent};
use leptos::prelude::*;
use web_sys;

use crate::components::focus_trap::use_focus_trap;

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
pub fn HelpModal(on_close: impl Fn(MouseEvent) + 'static + Clone) -> impl IntoView {
    let focus_trap = use_focus_trap();
    let on_close_clone = on_close.clone();

    // Combined keydown handler for focus trap and Escape key
    let handle_keydown = move |ev: KeyboardEvent| {
        // Handle Escape key to close modal
        if ev.key() == "Escape" {
            // Create a synthetic MouseEvent for on_close
            if let Ok(dummy_event) = web_sys::MouseEvent::new("click") {
                on_close_clone(dummy_event);
            }
            return;
        }

        // Handle Tab/Shift+Tab for focus trapping
        focus_trap(ev);
    };

    let on_close_overlay = on_close.clone();
    let on_close_button = move |ev: MouseEvent| {
        on_close(ev);
    };

    view! {
        <div class="help-overlay" on:click=move |ev| on_close_overlay(ev)>
        </div>
        <div class="help-modal" on:keydown=handle_keydown>
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
            <button class="close-btn" on:click=on_close_button>"Close"</button>
        </div>
    }
}
