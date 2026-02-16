use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{MessageEvent, Request, RequestInit, Response, WebSocket};

const API_BASE: &str = "http://localhost:9090";
const WS_BASE: &str = "ws://localhost:9090";

// ---------------------------------------------------------------------------
// API types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalInfo {
    pub id: String,
    pub title: String,
    pub status: String,
    pub cols: u16,
    pub rows: u16,
}

// ---------------------------------------------------------------------------
// API helpers
// ---------------------------------------------------------------------------

async fn api_create_terminal() -> Result<TerminalInfo, String> {
    let opts = RequestInit::new();
    opts.set_method("POST");

    let request = Request::new_with_str_and_init(&format!("{API_BASE}/api/terminals"), &opts)
        .map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or("no global window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;
    let json = JsFuture::from(resp.json().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    serde_wasm_bindgen::from_value(json).map_err(|e| format!("{e:?}"))
}

async fn api_list_terminals() -> Result<Vec<TerminalInfo>, String> {
    let opts = RequestInit::new();
    opts.set_method("GET");

    let request = Request::new_with_str_and_init(&format!("{API_BASE}/api/terminals"), &opts)
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or("no global window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;
    let json = JsFuture::from(resp.json().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    serde_wasm_bindgen::from_value(json).map_err(|e| format!("{e:?}"))
}

async fn api_delete_terminal(id: &str) -> Result<(), String> {
    let opts = RequestInit::new();
    opts.set_method("DELETE");

    let request =
        Request::new_with_str_and_init(&format!("{API_BASE}/api/terminals/{id}"), &opts)
            .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or("no global window")?;
    JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Terminal Panel Component
// ---------------------------------------------------------------------------

#[component]
fn TerminalPanel(
    info: TerminalInfo,
    on_close: Callback<String>,
) -> impl IntoView {
    let terminal_id = info.id.clone();
    let terminal_id_for_close = info.id.clone();
    let terminal_title = info.title.clone();

    let (output, set_output) = signal(String::new());
    let (connected, set_connected) = signal(false);
    let (input_value, set_input_value) = signal(String::new());

    // Use Rc<RefCell<>> for the WebSocket since JsValue is not Send+Sync.
    let ws_ref: Rc<RefCell<Option<WebSocket>>> = Rc::new(RefCell::new(None));
    let ws_ref_for_send = ws_ref.clone();

    // Connect WebSocket.
    let tid = terminal_id.clone();
    Effect::new(move |_| {
        let ws_url = format!("{WS_BASE}/ws/terminal/{tid}");
        let ws = match WebSocket::new(&ws_url) {
            Ok(ws) => ws,
            Err(e) => {
                set_output.update(|o| {
                    o.push_str(&format!("[error] Failed to connect: {e:?}\r\n"));
                });
                return;
            }
        };

        // On open.
        let set_connected_clone = set_connected;
        let on_open = Closure::<dyn FnMut()>::new(move || {
            set_connected_clone.set(true);
        });
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        on_open.forget();

        // On message.
        let set_output_clone = set_output;
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            if let Ok(text) = e.data().dyn_into::<js_sys::JsString>() {
                let text: String = text.into();
                set_output_clone.update(|o| {
                    o.push_str(&text);
                    // Keep buffer from growing too large.
                    if o.len() > 100_000 {
                        let start = o.len() - 80_000;
                        *o = o[start..].to_string();
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

    // Send input handler.
    let send_input = move || {
        let value = input_value.get_untracked();
        if value.is_empty() {
            return;
        }
        if let Some(ws) = ws_ref_for_send.borrow().as_ref() {
            let msg = serde_json::json!({"type": "input", "data": format!("{}\n", value)});
            let _ = ws.send_with_str(&msg.to_string());
        }
        set_input_value.set(String::new());
    };

    let send_input_clone = send_input.clone();

    // Auto-scroll effect.
    let output_ref = NodeRef::<leptos::html::Pre>::new();
    Effect::new(move |_| {
        let _ = output.get();
        if let Some(el) = output_ref.get() {
            let el: &web_sys::HtmlElement = &el;
            el.set_scroll_top(el.scroll_height());
        }
    });

    view! {
        <div class="terminal-panel">
            <div class="terminal-header">
                <span class="terminal-title">{terminal_title}</span>
                <span class="terminal-status" class:connected=connected>
                    {move || if connected.get() { "connected" } else { "disconnected" }}
                </span>
                <button
                    class="terminal-close-btn"
                    on:click=move |_| on_close.run(terminal_id_for_close.clone())
                >
                    "\u{2715}"
                </button>
            </div>
            <pre class="terminal-output" node_ref=output_ref>
                {move || output.get()}
            </pre>
            <div class="terminal-input-row">
                <span class="terminal-prompt">"$ "</span>
                <input
                    class="terminal-input"
                    type="text"
                    placeholder="Type a command..."
                    prop:value=input_value
                    on:input=move |ev| {
                        set_input_value.set(event_target_value(&ev));
                    }
                    on:keydown=move |ev| {
                        if ev.key() == "Enter" {
                            send_input_clone();
                        }
                    }
                />
            </div>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Terminals Page
// ---------------------------------------------------------------------------

#[component]
pub fn TerminalsPage() -> impl IntoView {
    let (terminals, set_terminals) = signal(Vec::<TerminalInfo>::new());
    let (_maximized, _set_maximized) = signal(None::<String>);
    let (error_msg, set_error_msg) = signal(None::<String>);

    // Load terminals on mount.
    Effect::new(move |_| {
        wasm_bindgen_futures::spawn_local(async move {
            match api_list_terminals().await {
                Ok(list) => set_terminals.set(list),
                Err(e) => set_error_msg.set(Some(format!("Failed to load terminals: {e}"))),
            }
        });
    });

    // Create terminal handler.
    let create_terminal = move |_| {
        wasm_bindgen_futures::spawn_local(async move {
            match api_create_terminal().await {
                Ok(info) => {
                    set_terminals.update(|list| list.push(info));
                    set_error_msg.set(None);
                }
                Err(e) => set_error_msg.set(Some(format!("Failed to create terminal: {e}"))),
            }
        });
    };

    // Close terminal handler.
    let close_terminal = Callback::new(move |id: String| {
        let id_clone = id.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = api_delete_terminal(&id_clone).await;
        });
        set_terminals.update(|list| list.retain(|t| t.id != id));
    });

    let terminal_count = move || terminals.get().len();

    view! {
        <div class="page-header">
            <h2>"Terminals"</h2>
            <div class="page-header-actions">
                <span class="terminal-count">
                    {move || format!("{}/4 terminals", terminal_count())}
                </span>
                <button
                    class="new-terminal-btn"
                    on:click=create_terminal
                    disabled=move || terminal_count() >= 4
                >
                    "+ New Terminal"
                </button>
            </div>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="terminal-error">{msg}</div>
        })}

        <div class="terminal-grid">
            {move || terminals.get().into_iter().map(|info| {
                let on_close = close_terminal.clone();
                view! {
                    <TerminalPanel
                        info=info
                        on_close=on_close
                    />
                }
            }).collect::<Vec<_>>()}
        </div>

        {move || terminals.get().is_empty().then(|| view! {
            <div class="terminal-empty">
                <div class="terminal-empty-icon">"\u{1F5A5}\u{FE0F}"</div>
                <div class="terminal-empty-text">"No terminals running"</div>
                <div class="terminal-empty-hint">"Click \"+ New Terminal\" to start a shell session"</div>
            </div>
        })}
    }
}
