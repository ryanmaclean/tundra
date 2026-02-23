use crate::api;
use leptos::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket};

/// A single terminal emulator pane that connects via WebSocket and renders
/// output line-by-line with a blinking cursor and input bar.
#[component]
pub fn TerminalView(
    #[prop()] terminal_id: String,
    #[prop()] terminal_title: String,
    #[prop(default = 80)] cols: u32,
    #[prop(default = 24)] rows: u32,
    #[prop()] on_close: Callback<String>,
) -> impl IntoView {
    let terminal_id_ws = terminal_id.clone();
    let terminal_id_close = terminal_id.clone();

    let (output_lines, set_output_lines) = signal(Vec::<String>::new());
    let (connected, set_connected) = signal(false);
    let (input_text, set_input_text) = signal(String::new());
    let screen_ref = NodeRef::<leptos::html::Div>::new();

    // WebSocket handle shared via Rc<RefCell<>>.
    let ws_ref: Rc<RefCell<Option<WebSocket>>> = Rc::new(RefCell::new(None));
    let ws_ref_for_send = ws_ref.clone();
    let ws_ref_for_cleanup = ws_ref.clone();

    // Connect WebSocket on mount.
    Effect::new(move |_| {
        let base = api::get_api_base();
        let ws_base = base
            .replace("http://", "ws://")
            .replace("https://", "wss://");
        let ws_url = format!("{ws_base}/ws/terminal/{terminal_id_ws}");
        let ws = match WebSocket::new(&ws_url) {
            Ok(ws) => ws,
            Err(e) => {
                set_output_lines.update(|lines| {
                    lines.push(format!("[error] Failed to connect: {e:?}"));
                });
                return;
            }
        };

        // On open.
        let on_open = Closure::<dyn FnMut()>::new(move || {
            set_connected.set(true);
        });
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        on_open.forget();

        // On message — split incoming data into lines.
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            if let Ok(text) = e.data().dyn_into::<js_sys::JsString>() {
                let text: String = text.into();
                set_output_lines.update(|lines| {
                    for line in text.split('\n') {
                        // Handle \r\n by trimming trailing \r.
                        let cleaned = line.strip_suffix('\r').unwrap_or(line);
                        lines.push(cleaned.to_string());
                    }
                    // Cap at 2000 lines to avoid unbounded growth.
                    if lines.len() > 2000 {
                        let drain_count = lines.len() - 1500;
                        lines.drain(..drain_count);
                    }
                });
            }
        });
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();

        // On close.
        let set_connected_close = set_connected;
        let on_close = Closure::<dyn FnMut()>::new(move || {
            set_connected_close.set(false);
        });
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        on_close.forget();

        *ws_ref.borrow_mut() = Some(ws);
    });

    // Close the WebSocket when the component unmounts to prevent leaks.
    // SendWrapper is needed because Rc<RefCell> is not Send+Sync, but WASM is single-threaded.
    let ws_ref_cleanup = send_wrapper::SendWrapper::new(ws_ref_for_cleanup);
    on_cleanup(move || {
        if let Some(ws) = ws_ref_cleanup.borrow().as_ref() {
            ws.close().ok();
        }
    });

    // Auto-scroll when output changes.
    Effect::new(move |_| {
        let _ = output_lines.get();
        if let Some(el) = screen_ref.get() {
            let el: &web_sys::HtmlElement = &el;
            el.set_scroll_top(el.scroll_height());
        }
    });

    // Send input via WebSocket.
    let send_input = move || {
        let value = input_text.get_untracked();
        if value.is_empty() {
            return;
        }
        if let Some(ws) = ws_ref_for_send.borrow().as_ref() {
            let msg = serde_json::json!({"type": "input", "data": format!("{}\n", value)});
            let _ = ws.send_with_str(&msg.to_string());
        }
        set_input_text.set(String::new());
    };

    let send_input_key = send_input.clone();

    view! {
        <div class="terminal-emulator">
            <div class="terminal-pane-header">
                <span class="terminal-title">{terminal_title}</span>
                <span class=(move || {
                    if connected.get() {
                        "terminal-status-connected"
                    } else {
                        "terminal-status-disconnected"
                    }
                })>
                    {move || if connected.get() { "\u{25CF} Connected" } else { "\u{25CB} Disconnected" }}
                </span>
                <span class="terminal-dimensions">{cols}{"\u{00D7}"}{rows}</span>
                <button
                    class="terminal-close-btn"
                    on:click=move |_| on_close.run(terminal_id_close.clone())
                >
                    "\u{2715}"
                </button>
            </div>
            <div class="terminal-screen" node_ref=screen_ref>
                <For
                    each=move || {
                        output_lines.get().into_iter().enumerate().collect::<Vec<_>>()
                    }
                    key=|(i, _)| *i
                    children=move |(_, line)| {
                        view! {
                            <div class="terminal-line">{line}</div>
                        }
                    }
                />
                <div class="terminal-cursor">{" "}</div>
            </div>
            <div class="terminal-input-bar">
                <span class="terminal-prompt">{"$"}</span>
                <input
                    type="text"
                    class="terminal-input"
                    placeholder="Type a command..."
                    prop:value=move || input_text.get()
                    on:input=move |ev| set_input_text.set(event_target_value(&ev))
                    on:keydown=move |ev| {
                        if ev.key() == "Enter" {
                            send_input_key();
                        }
                    }
                />
            </div>
        </div>
    }
}
