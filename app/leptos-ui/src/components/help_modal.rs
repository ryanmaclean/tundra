use leptos::prelude::*;
use leptos::ev::MouseEvent;

#[component]
pub fn HelpModal(
    on_close: impl Fn(MouseEvent) + 'static,
) -> impl IntoView {
    view! {
        <div class="help-overlay" on:click=on_close>
        </div>
        <div class="help-modal">
            <h2>"Keyboard Shortcuts"</h2>
            <div class="keybind">
                <span>"Show help"</span>
                <kbd>"?"</kbd>
            </div>
            <div class="keybind">
                <span>"Dashboard"</span>
                <kbd>"1"</kbd>
            </div>
            <div class="keybind">
                <span>"Agents"</span>
                <kbd>"2"</kbd>
            </div>
            <div class="keybind">
                <span>"Beads"</span>
                <kbd>"3"</kbd>
            </div>
            <div class="keybind">
                <span>"Sessions"</span>
                <kbd>"4"</kbd>
            </div>
            <div class="keybind">
                <span>"Convoys"</span>
                <kbd>"5"</kbd>
            </div>
            <div class="keybind">
                <span>"Costs"</span>
                <kbd>"6"</kbd>
            </div>
            <div class="keybind">
                <span>"Analytics"</span>
                <kbd>"7"</kbd>
            </div>
            <div class="keybind">
                <span>"Config"</span>
                <kbd>"8"</kbd>
            </div>
            <div class="keybind">
                <span>"MCP"</span>
                <kbd>"9"</kbd>
            </div>
            <button class="close-btn">"Close"</button>
        </div>
    }
}
