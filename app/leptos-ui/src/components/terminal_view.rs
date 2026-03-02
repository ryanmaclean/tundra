use crate::api;
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlElement, MessageEvent, Request, RequestInit, Response, WebSocket};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window, js_name = tundraCreateTerminal)]
    fn js_create_terminal(container: &HtmlElement, options: &JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = window, js_name = tundraAttachOnData)]
    fn js_attach_on_data(handle: &str, cb: &js_sys::Function) -> bool;

    #[wasm_bindgen(js_namespace = window, js_name = tundraAttachOnResize)]
    fn js_attach_on_resize(handle: &str, cb: &js_sys::Function) -> bool;

    #[wasm_bindgen(js_namespace = window, js_name = tundraWriteTerminal)]
    fn js_write_terminal(handle: &str, data: &str) -> bool;

    #[wasm_bindgen(js_namespace = window, js_name = tundraFitTerminal)]
    fn js_fit_terminal(handle: &str) -> JsValue;

    #[wasm_bindgen(js_namespace = window, js_name = tundraFocusTerminal)]
    fn js_focus_terminal(handle: &str) -> bool;

    #[wasm_bindgen(js_namespace = window, js_name = tundraDisposeTerminal)]
    fn js_dispose_terminal(handle: &str) -> bool;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalProfile {
    BundledCard,
}

impl TerminalProfile {
    fn from_name(name: &str) -> Self {
        match name {
            "bundled-card" => TerminalProfile::BundledCard,
            _ => TerminalProfile::BundledCard,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            TerminalProfile::BundledCard => "bundled-card",
        }
    }

    fn font_family(self) -> &'static str {
        match self {
            TerminalProfile::BundledCard => {
                "\"Iosevka Term\",\"JetBrains Mono\",\"SF Mono\",\"Menlo\",monospace"
            }
        }
    }

    fn font_size(self, requested: u16) -> u16 {
        match self {
            TerminalProfile::BundledCard => requested.clamp(10, 13),
        }
    }

    fn line_height(self) -> f64 {
        match self {
            TerminalProfile::BundledCard => 1.02,
        }
    }

    fn letter_spacing(self) -> f64 {
        match self {
            TerminalProfile::BundledCard => 0.15,
        }
    }
}

async fn api_patch_terminal_settings(
    terminal_id: &str,
    profile: TerminalProfile,
    font_size: u16,
    cursor_style: &str,
    cursor_blink: bool,
) -> Result<(), String> {
    let opts = RequestInit::new();
    opts.set_method("PATCH");

    let payload = serde_json::json!({
        "profile": profile.as_str(),
        "font_size": font_size,
        "cursor_style": cursor_style,
        "cursor_blink": cursor_blink,
        "font_family": profile.font_family(),
        "line_height": profile.line_height(),
        "letter_spacing": profile.letter_spacing(),
    });
    let body =
        serde_wasm_bindgen::to_value(&payload).map_err(|e| format!("serialize settings: {e:?}"))?;
    opts.set_body(&body);

    let api_base = api::get_api_base();
    let request = Request::new_with_str_and_init(
        &format!("{api_base}/api/terminals/{terminal_id}/settings"),
        &opts,
    )
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

    if resp.ok() {
        Ok(())
    } else {
        Err(format!("settings patch failed: HTTP {}", resp.status()))
    }
}

fn send_ws_json(ws: &WebSocket, value: serde_json::Value) {
    let _ = ws.send_with_str(&value.to_string());
}

fn send_resize_if_available(ws: &WebSocket, term_handle: &str) {
    let fit = js_fit_terminal(term_handle);
    if fit.is_null() || fit.is_undefined() {
        return;
    }
    let cols = js_sys::Reflect::get(&fit, &JsValue::from_str("cols"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(80.0) as u16;
    let rows = js_sys::Reflect::get(&fit, &JsValue::from_str("rows"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(24.0) as u16;
    send_ws_json(
        ws,
        serde_json::json!({
            "type": "resize",
            "cols": cols,
            "rows": rows,
        }),
    );
}

#[component]
pub fn TerminalView(
    #[prop()] terminal_id: String,
    #[prop()] terminal_title: String,
    #[prop(default = 80)] cols: u32,
    #[prop(default = 24)] rows: u32,
    #[prop(default = 12)] font_size: u16,
    #[prop(default = "\"Iosevka Term\",\"JetBrains Mono\",\"SF Mono\",\"Menlo\",monospace".to_string())]
    font_family: String,
    #[prop(default = 1.02)] line_height: f32,
    #[prop(default = 0.15)] letter_spacing: f32,
    #[prop(default = "bundled-card".to_string())] profile_name: String,
    #[prop(default = "block".to_string())] cursor_style: String,
    #[prop(default = true)] cursor_blink: bool,
    #[prop()] on_close: Callback<String>,
) -> impl IntoView {
    let (connected, set_connected) = signal(false);
    let (init_error, set_init_error) = signal(None::<String>);
    let (initialized, set_initialized) = signal(false);

    let profile = TerminalProfile::from_name(&profile_name);
    let container_ref = NodeRef::<leptos::html::Div>::new();

    let term_handle_ref: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let ws_ref: Rc<RefCell<Option<WebSocket>>> = Rc::new(RefCell::new(None));
    let on_data_ref: Rc<RefCell<Option<Closure<dyn FnMut(String)>>>> = Rc::new(RefCell::new(None));
    let on_resize_ref: Rc<RefCell<Option<Closure<dyn FnMut(u16, u16)>>>> =
        Rc::new(RefCell::new(None));

    // Initialize xterm and websocket once the container is mounted.
    {
        let term_handle_ref = term_handle_ref.clone();
        let ws_ref = ws_ref.clone();
        let on_data_ref = on_data_ref.clone();
        let on_resize_ref = on_resize_ref.clone();
        let terminal_id_ws = terminal_id.clone();
        let cursor_style_ws = cursor_style.clone();
        Effect::new(move |_| {
            if initialized.get() {
                return;
            }
            let Some(container) = container_ref.get() else {
                return;
            };

            let options = js_sys::Object::new();
            let clamped_font_size = profile.font_size(font_size);
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("fontFamily"),
                &JsValue::from_str(if font_family.trim().is_empty() {
                    profile.font_family()
                } else {
                    &font_family
                }),
            );
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("fontSize"),
                &JsValue::from_f64(clamped_font_size as f64),
            );
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("lineHeight"),
                &JsValue::from_f64(if line_height <= 0.0 {
                    profile.line_height()
                } else {
                    line_height as f64
                }),
            );
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("letterSpacing"),
                &JsValue::from_f64(if letter_spacing < 0.0 {
                    profile.letter_spacing()
                } else {
                    letter_spacing as f64
                }),
            );
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("cursorStyle"),
                &JsValue::from_str(&cursor_style_ws),
            );
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("cursorBlink"),
                &JsValue::from_bool(cursor_blink),
            );

            let handle_js = js_create_terminal(&container, &options.into());
            let Some(term_handle) = handle_js.as_string() else {
                set_init_error.set(Some(
                    "terminal emulator runtime unavailable (xterm not loaded)".to_string(),
                ));
                return;
            };

            *term_handle_ref.borrow_mut() = Some(term_handle.clone());
            let _ = js_focus_terminal(&term_handle);

            // Attach keyboard input callback (xterm -> websocket input).
            let ws_ref_input = ws_ref.clone();
            let on_data = Closure::<dyn FnMut(String)>::new(move |data: String| {
                if let Some(ws) = ws_ref_input.borrow().as_ref() {
                    send_ws_json(
                        ws,
                        serde_json::json!({
                            "type": "input",
                            "data": data,
                        }),
                    );
                }
            });
            let _ = js_attach_on_data(&term_handle, on_data.as_ref().unchecked_ref());
            *on_data_ref.borrow_mut() = Some(on_data);

            // Attach resize callback (xterm fit -> websocket resize).
            let ws_ref_resize = ws_ref.clone();
            let on_resize = Closure::<dyn FnMut(u16, u16)>::new(move |cols: u16, rows: u16| {
                if let Some(ws) = ws_ref_resize.borrow().as_ref() {
                    send_ws_json(
                        ws,
                        serde_json::json!({
                            "type": "resize",
                            "cols": cols,
                            "rows": rows,
                        }),
                    );
                }
            });
            let _ = js_attach_on_resize(&term_handle, on_resize.as_ref().unchecked_ref());
            *on_resize_ref.borrow_mut() = Some(on_resize);

            // Connect websocket.
            let base = api::get_api_base();
            let ws_base = base
                .replace("http://", "ws://")
                .replace("https://", "wss://");
            let ws_url = format!("{ws_base}/ws/terminal/{terminal_id_ws}");
            let ws = match WebSocket::new(&ws_url) {
                Ok(ws) => ws,
                Err(e) => {
                    set_init_error
                        .set(Some(format!("failed to connect terminal websocket: {e:?}")));
                    return;
                }
            };

            let term_handle_open = term_handle.clone();
            let ws_ref_for_open = ws_ref.clone();
            let on_open = Closure::<dyn FnMut()>::new(move || {
                set_connected.set(true);
                if let Some(ws) = ws_ref_for_open.borrow().as_ref() {
                    send_resize_if_available(ws, &term_handle_open);
                }
            });
            ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
            on_open.forget();

            let term_handle_msg = term_handle.clone();
            let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
                if let Ok(text) = e.data().dyn_into::<js_sys::JsString>() {
                    let _ = js_write_terminal(&term_handle_msg, &String::from(text));
                }
            });
            ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
            on_message.forget();

            let on_close = Closure::<dyn FnMut()>::new(move || {
                set_connected.set(false);
            });
            ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
            on_close.forget();

            *ws_ref.borrow_mut() = Some(ws);

            // Persist profile settings server-side for this terminal.
            let terminal_id_settings = terminal_id_ws.clone();
            let cursor_style_for_patch = cursor_style_ws.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = api_patch_terminal_settings(
                    &terminal_id_settings,
                    profile,
                    clamped_font_size,
                    &cursor_style_for_patch,
                    cursor_blink,
                )
                .await;
            });

            set_initialized.set(true);
        });
    }

    // Cleanup websocket + xterm on unmount.
    let ws_ref_cleanup = SendWrapper::new(ws_ref.clone());
    let term_handle_cleanup = SendWrapper::new(term_handle_ref.clone());
    let on_data_cleanup = SendWrapper::new(on_data_ref.clone());
    let on_resize_cleanup = SendWrapper::new(on_resize_ref.clone());
    on_cleanup(move || {
        if let Some(ws) = ws_ref_cleanup.borrow().as_ref() {
            let _ = ws.close();
        }
        *ws_ref_cleanup.borrow_mut() = None;
        *on_data_cleanup.borrow_mut() = None;
        *on_resize_cleanup.borrow_mut() = None;

        if let Some(handle) = term_handle_cleanup.borrow_mut().take() {
            let _ = js_dispose_terminal(&handle);
        }
    });

    view! {
        <div class="terminal-emulator terminal-profile-card">
            <div class="terminal-pane-header">
                <span class="terminal-title">{terminal_title}</span>
                <span class="terminal-profile-badge">{profile.as_str()}</span>
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
                    on:click=move |_| on_close.run(terminal_id.clone())
                >
                    "\u{2715}"
                </button>
            </div>

            {move || init_error.get().map(|e| view! {
                <div class="terminal-error">{e}</div>
            })}

            <div class="terminal-screen terminal-screen-xterm" node_ref=container_ref></div>
        </div>
    }
}
