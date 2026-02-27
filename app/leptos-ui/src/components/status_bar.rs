use leptos::prelude::*;

use crate::state::use_app_state;
use crate::themed::{format_status_full, themed, Prompt};

#[component]
pub fn StatusBar(#[prop(into)] on_help: Callback<()>) -> impl IntoView {
    let state = use_app_state();
    let status = state.status;
    let mode = state.display_mode;

    // Live-ticking uptime: capture the server-reported uptime and increment locally.
    let (local_tick, set_local_tick) = signal(0u64);

    // Every second, bump the local tick counter.
    Effect::new(move |_| {
        use wasm_bindgen::closure::Closure;
        use wasm_bindgen::JsCast;
        let window = web_sys::window().unwrap();
        let cb = Closure::wrap(Box::new(move || {
            set_local_tick.update(|t| *t += 1);
        }) as Box<dyn FnMut()>);
        let _ = window.set_interval_with_callback_and_timeout_and_arguments_0(
            cb.as_ref().unchecked_ref(),
            1000,
        );
        cb.forget(); // leak the closure — it lives for the app lifetime
    });

    let uptime = move || {
        let base = status.get().uptime_secs;
        let secs = base + local_tick.get();
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        format!("{:02}:{:02}:{:02}", h, m, s)
    };

    view! {
        <div class="status-bar">
            <div class="left">
                <span aria-live="polite">
                    {move || {
                        let dot_svg = if status.get().daemon_running {
                            r##"<svg width="8" height="8" viewBox="0 0 8 8"><circle cx="4" cy="4" r="3" fill="#22c55e"><animate attributeName="r" values="3;3.5;3" dur="2s" repeatCount="indefinite"/></circle><circle cx="4" cy="4" r="3" fill="none" stroke="#22c55e" stroke-width="0.5" opacity="0.4"><animate attributeName="r" values="3;5;3" dur="2s" repeatCount="indefinite"/><animate attributeName="opacity" values="0.4;0;0.4" dur="2s" repeatCount="indefinite"/></circle></svg>"##
                        } else {
                            r##"<svg width="8" height="8" viewBox="0 0 8 8"><circle cx="4" cy="4" r="3" fill="#ef4444"/></svg>"##
                        };
                        let label = if status.get().daemon_running { "daemon: running" } else { "daemon: stopped" };
                        view! {
                            <span class="status-dot-svg" inner_html=dot_svg></span>
                            {label}
                        }
                    }}
                </span>
                <span aria-live="polite">{move || {
                    let m = mode.get();
                    let s = status.get();
                    format_status_full(m, s.active_agents as usize, s.total_beads as usize, &uptime())
                }}</span>
                {move || {
                    if state.is_demo.get() {
                        let badge_class = match mode.get() {
                            crate::state::DisplayMode::Foil => "demo-badge demo-badge-foil",
                            crate::state::DisplayMode::Vt100 => "demo-badge demo-badge-vt100",
                            crate::state::DisplayMode::Standard => "demo-badge",
                        };
                        let label = match mode.get() {
                            crate::state::DisplayMode::Vt100 => "[DEMO MODE]",
                            _ => "DEMO",
                        };
                        view! { <span class=badge_class>{label}</span> }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }
                }}
            </div>
            <div class="right">
                <span>{move || format!("{}: {}", themed(mode.get(), Prompt::StatusUptime), uptime())}</span>
                <span
                    style="cursor: pointer;"
                    on:click=move |_| {
                        on_help.run(());
                    }
                ><kbd>"?"</kbd>" help"</span>
            </div>
        </div>
    }
}
