use crate::api;
use crate::events::reconnect_delay_secs;
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{CloseEvent, HtmlElement, MessageEvent, RequestInit, Response, WebSocket};

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

/// Connection state of a terminal's WebSocket, surfaced in the pane header
/// (and as `data-conn-state` / `data-reconnect-attempt` attributes so agents
/// and browser tests can read it without parsing the label).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalConnState {
    /// First connection attempt in flight.
    Connecting,
    /// WebSocket open; input/output flowing.
    Connected,
    /// Connection lost; retry `attempt` of `max_attempts` is scheduled or in flight.
    Reconnecting { attempt: u32, max_attempts: u32 },
    /// Retries exhausted (or never started); manual reconnect required.
    Disconnected,
}

impl TerminalConnState {
    /// Stable machine-readable name (value of `data-conn-state`).
    pub fn as_str(self) -> &'static str {
        match self {
            TerminalConnState::Connecting => "connecting",
            TerminalConnState::Connected => "connected",
            TerminalConnState::Reconnecting { .. } => "reconnecting",
            TerminalConnState::Disconnected => "disconnected",
        }
    }

    /// CSS class for the header status badge.
    pub fn css_class(self) -> &'static str {
        match self {
            TerminalConnState::Connected => "terminal-status-connected",
            TerminalConnState::Connecting | TerminalConnState::Reconnecting { .. } => {
                "terminal-status-reconnecting"
            }
            TerminalConnState::Disconnected => "terminal-status-disconnected",
        }
    }

    /// Human label for the header status badge.
    pub fn label(self) -> String {
        match self {
            TerminalConnState::Connecting => "\u{25CC} Connecting\u{2026}".to_string(),
            TerminalConnState::Connected => "\u{25CF} Connected".to_string(),
            TerminalConnState::Reconnecting {
                attempt,
                max_attempts,
            } => format!("\u{21BB} Reconnecting\u{2026} ({attempt}/{max_attempts})"),
            TerminalConnState::Disconnected => "\u{25CB} Disconnected".to_string(),
        }
    }

    /// Current reconnect attempt, if reconnecting.
    pub fn attempt(self) -> Option<u32> {
        match self {
            TerminalConnState::Reconnecting { attempt, .. } => Some(attempt),
            _ => None,
        }
    }
}

/// Exponential-backoff bookkeeping for the terminal WebSocket.
///
/// Delays follow [`crate::events::reconnect_delay_secs`] (1, 2, 4, 8, 16, 16…
/// seconds). The daemon keeps a disconnected PTY alive for a 30 s grace
/// period and replays buffered output on reconnect, then answers 410 Gone;
/// retrying forever after that only burns requests, so after `max_attempts`
/// consecutive failures the helper gives up and the UI offers a manual retry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconnectBackoff {
    failures: u32,
    max_attempts: u32,
}

impl Default for ReconnectBackoff {
    fn default() -> Self {
        Self::new(Self::DEFAULT_MAX_ATTEMPTS)
    }
}

impl ReconnectBackoff {
    /// 1+2+4+8+16+16+16+16 = 79 s of retries, well past the 30 s server grace.
    pub const DEFAULT_MAX_ATTEMPTS: u32 = 8;

    pub fn new(max_attempts: u32) -> Self {
        Self {
            failures: 0,
            max_attempts,
        }
    }

    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// Consecutive failures since the last successful open.
    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// A connection opened: reset the backoff.
    pub fn on_open(&mut self) {
        self.failures = 0;
    }

    /// Manual retry: forget previous failures.
    pub fn reset(&mut self) {
        self.failures = 0;
    }

    /// A connection closed or failed to start. Returns `(attempt, delay_secs)`
    /// for the next reconnect, or `None` once `max_attempts` is exhausted.
    pub fn on_failure(&mut self) -> Option<(u32, u32)> {
        self.failures = self.failures.saturating_add(1);
        if self.failures > self.max_attempts {
            None
        } else {
            Some((self.failures, reconnect_delay_secs(self.failures)))
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
    let request = crate::api::new_request(
        &format!("{api_base}/api/terminals/{terminal_id}/settings"),
        &opts,
    )?;
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

/// Live WebSocket plus the JS callbacks bound to it. Callbacks are owned here
/// (not `forget()`-ed) so reconnect cycles do not leak closures; they are
/// unbound from the socket before being dropped.
struct BoundSocket {
    ws: WebSocket,
    _on_open: Closure<dyn FnMut()>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
}

impl BoundSocket {
    /// Unbind callbacks (so a late `close` event cannot trigger a reconnect or
    /// call a dropped closure) and close the socket.
    fn shutdown(self) {
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onclose(None);
        let _ = self.ws.close();
    }
}

/// Shared state for one terminal pane's WebSocket lifecycle.
struct TerminalWs {
    terminal_id: String,
    term_handle: Rc<RefCell<Option<String>>>,
    /// Current socket, read by the xterm input/resize callbacks.
    ws: Rc<RefCell<Option<WebSocket>>>,
    bound: RefCell<Option<BoundSocket>>,
    backoff: RefCell<ReconnectBackoff>,
    mounted: Cell<bool>,
    set_state: WriteSignal<TerminalConnState>,
}

impl TerminalWs {
    fn detach(&self) {
        *self.ws.borrow_mut() = None;
        if let Some(bound) = self.bound.borrow_mut().take() {
            bound.shutdown();
        }
    }
}

/// Daemon path of a terminal's WebSocket (`GET /ws/terminal/{id}`).
fn terminal_ws_path(terminal_id: &str) -> String {
    format!("/ws/terminal/{terminal_id}")
}

/// Open (or re-open) the terminal WebSocket. The URL is rebuilt on every
/// attempt via [`api::ws_url`], so the `api_key` query param is re-sent.
fn connect_terminal_ws(ctx: &Rc<TerminalWs>) {
    if !ctx.mounted.get() {
        return;
    }
    let Some(term_handle) = ctx.term_handle.borrow().clone() else {
        return;
    };
    ctx.detach();

    let ws_url = api::ws_url(&terminal_ws_path(&ctx.terminal_id));
    let ws = match WebSocket::new(&ws_url) {
        Ok(ws) => ws,
        Err(e) => {
            web_sys::console::warn_1(
                &format!("[terminal] failed to open websocket: {e:?}").into(),
            );
            schedule_terminal_reconnect(ctx);
            return;
        }
    };

    let ctx_open = Rc::downgrade(ctx);
    let term_handle_open = term_handle.clone();
    let on_open = Closure::<dyn FnMut()>::new(move || {
        let Some(ctx) = ctx_open.upgrade() else {
            return;
        };
        ctx.backoff.borrow_mut().on_open();
        ctx.set_state.set(TerminalConnState::Connected);
        let ws = ctx.ws.borrow().clone();
        if let Some(ws) = ws {
            send_resize_if_available(&ws, &term_handle_open);
        }
    });
    ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

    let term_handle_msg = term_handle;
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
        if let Ok(text) = e.data().dyn_into::<js_sys::JsString>() {
            let _ = js_write_terminal(&term_handle_msg, &String::from(text));
        }
    });
    ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

    // A close before `open` (e.g. 404/410 on upgrade) also lands here and
    // counts as a failure, so the delay grows instead of hammering the daemon.
    let ctx_close = Rc::downgrade(ctx);
    let on_close = Closure::<dyn FnMut(CloseEvent)>::new(move |_: CloseEvent| {
        let Some(ctx) = ctx_close.upgrade() else {
            return;
        };
        *ctx.ws.borrow_mut() = None;
        schedule_terminal_reconnect(&ctx);
    });
    ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

    *ctx.ws.borrow_mut() = Some(ws.clone());
    *ctx.bound.borrow_mut() = Some(BoundSocket {
        ws,
        _on_open: on_open,
        _on_message: on_message,
        _on_close: on_close,
    });
}

/// Schedule the next reconnect with exponential backoff, or give up.
fn schedule_terminal_reconnect(ctx: &Rc<TerminalWs>) {
    if !ctx.mounted.get() {
        return;
    }
    let next = ctx.backoff.borrow_mut().on_failure();
    let Some((attempt, delay_secs)) = next else {
        ctx.set_state.set(TerminalConnState::Disconnected);
        web_sys::console::warn_1(
            &format!(
                "[terminal] {} websocket: giving up after {} attempts",
                ctx.terminal_id,
                ctx.backoff.borrow().max_attempts()
            )
            .into(),
        );
        return;
    };
    ctx.set_state.set(TerminalConnState::Reconnecting {
        attempt,
        max_attempts: ctx.backoff.borrow().max_attempts(),
    });
    let ctx = Rc::downgrade(ctx);
    wasm_bindgen_futures::spawn_local(async move {
        gloo_timers::future::TimeoutFuture::new(delay_secs.saturating_mul(1000)).await;
        if let Some(ctx) = ctx.upgrade() {
            connect_terminal_ws(&ctx);
        }
    });
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
    let (conn_state, set_conn_state) = signal(TerminalConnState::Connecting);
    let (init_error, set_init_error) = signal(None::<String>);
    let (initialized, set_initialized) = signal(false);

    let profile = TerminalProfile::from_name(&profile_name);
    let container_ref = NodeRef::<leptos::html::Div>::new();

    let term_handle_ref: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let ws_ref: Rc<RefCell<Option<WebSocket>>> = Rc::new(RefCell::new(None));
    let on_data_ref: Rc<RefCell<Option<Closure<dyn FnMut(String)>>>> = Rc::new(RefCell::new(None));
    let on_resize_ref: Rc<RefCell<Option<Closure<dyn FnMut(u16, u16)>>>> =
        Rc::new(RefCell::new(None));

    let ws_ctx = Rc::new(TerminalWs {
        terminal_id: terminal_id.clone(),
        term_handle: term_handle_ref.clone(),
        ws: ws_ref.clone(),
        bound: RefCell::new(None),
        backoff: RefCell::new(ReconnectBackoff::default()),
        mounted: Cell::new(true),
        set_state: set_conn_state,
    });

    // Initialize xterm and websocket once the container is mounted.
    {
        let term_handle_ref = term_handle_ref.clone();
        let ws_ref = ws_ref.clone();
        let on_data_ref = on_data_ref.clone();
        let on_resize_ref = on_resize_ref.clone();
        let ws_ctx = ws_ctx.clone();
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
                set_conn_state.set(TerminalConnState::Disconnected);
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

            // Connect websocket (reconnects with exponential backoff on close).
            connect_terminal_ws(&ws_ctx);

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

    // Manual reconnect once automatic retries are exhausted.
    let ws_ctx_retry = SendWrapper::new(ws_ctx.clone());
    let on_retry = move |_: web_sys::MouseEvent| {
        ws_ctx_retry.backoff.borrow_mut().reset();
        ws_ctx_retry.set_state.set(TerminalConnState::Connecting);
        connect_terminal_ws(&ws_ctx_retry);
    };

    // Cleanup websocket + xterm on unmount. `mounted = false` stops any
    // pending backoff timer from reconnecting; `detach` unbinds the close
    // handler before closing so the close does not schedule a retry.
    let ws_ctx_cleanup = SendWrapper::new(ws_ctx);
    let term_handle_cleanup = SendWrapper::new(term_handle_ref.clone());
    let on_data_cleanup = SendWrapper::new(on_data_ref.clone());
    let on_resize_cleanup = SendWrapper::new(on_resize_ref.clone());
    on_cleanup(move || {
        ws_ctx_cleanup.mounted.set(false);
        ws_ctx_cleanup.detach();
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
                <span
                    class=(move || conn_state.get().css_class())
                    role="status"
                    aria-live="polite"
                    data-conn-state=move || conn_state.get().as_str()
                    data-reconnect-attempt=move || conn_state.get().attempt().map(|a| a.to_string())
                >
                    {move || conn_state.get().label()}
                </span>
                {move || {
                    (initialized.get() && conn_state.get() == TerminalConnState::Disconnected)
                        .then(|| {
                            let on_retry = on_retry.clone();
                            view! {
                                <button
                                    class="terminal-reconnect-btn"
                                    title="Reconnect terminal"
                                    on:click=on_retry
                                >
                                    "\u{21BB} Reconnect"
                                </button>
                            }
                        })
                }}
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod reconnect_tests {
    use super::{terminal_ws_path, ReconnectBackoff, TerminalConnState};
    use crate::api::ws_url_for;

    #[test]
    fn reconnect_url_carries_api_key_every_attempt() {
        // connect_terminal_ws rebuilds the URL per attempt from this path.
        let path = terminal_ws_path("t-1");
        assert_eq!(path, "/ws/terminal/t-1");
        assert_eq!(
            ws_url_for("http://127.0.0.1:9090", &path, Some("k%2B1")),
            "ws://127.0.0.1:9090/ws/terminal/t-1?api_key=k%2B1"
        );
    }

    #[test]
    fn backoff_follows_events_schedule_then_gives_up() {
        let mut b = ReconnectBackoff::new(7);
        let plan: Vec<_> = std::iter::from_fn(|| b.on_failure()).collect();
        assert_eq!(
            plan,
            vec![(1, 1), (2, 2), (3, 4), (4, 8), (5, 16), (6, 16), (7, 16)]
        );
        // Stays exhausted until reset.
        assert_eq!(b.on_failure(), None);
        assert_eq!(b.on_failure(), None);
    }

    #[test]
    fn default_backoff_outlasts_server_grace_period() {
        let mut b = ReconnectBackoff::default();
        assert_eq!(b.max_attempts(), ReconnectBackoff::DEFAULT_MAX_ATTEMPTS);
        let mut total = 0;
        while let Some((_, delay)) = b.on_failure() {
            total += delay;
        }
        // Daemon keeps disconnected PTYs for 30 s (WS_RECONNECT_GRACE).
        assert!(total > 30, "retries span {total}s, must exceed 30s grace");
        assert_eq!(b.failures(), ReconnectBackoff::DEFAULT_MAX_ATTEMPTS + 1);
    }

    #[test]
    fn open_resets_backoff_to_first_delay() {
        let mut b = ReconnectBackoff::new(8);
        b.on_failure();
        b.on_failure();
        assert_eq!(b.on_failure(), Some((3, 4)));
        b.on_open();
        assert_eq!(b.failures(), 0);
        assert_eq!(b.on_failure(), Some((1, 1)));
    }

    #[test]
    fn manual_reset_revives_exhausted_backoff() {
        let mut b = ReconnectBackoff::new(1);
        assert_eq!(b.on_failure(), Some((1, 1)));
        assert_eq!(b.on_failure(), None);
        b.reset();
        assert_eq!(b.on_failure(), Some((1, 1)));
    }

    #[test]
    fn zero_max_attempts_never_retries() {
        let mut b = ReconnectBackoff::new(0);
        assert_eq!(b.on_failure(), None);
    }

    #[test]
    fn conn_state_surfaces_reconnecting() {
        let r = TerminalConnState::Reconnecting {
            attempt: 3,
            max_attempts: 8,
        };
        assert_eq!(r.as_str(), "reconnecting");
        assert_eq!(r.css_class(), "terminal-status-reconnecting");
        assert!(r.label().contains("Reconnecting"));
        assert!(r.label().contains("(3/8)"));
        assert_eq!(r.attempt(), Some(3));

        assert_eq!(TerminalConnState::Connected.as_str(), "connected");
        assert_eq!(
            TerminalConnState::Connected.css_class(),
            "terminal-status-connected"
        );
        assert_eq!(TerminalConnState::Connected.attempt(), None);
        assert_eq!(TerminalConnState::Connecting.as_str(), "connecting");
        assert_eq!(
            TerminalConnState::Connecting.css_class(),
            "terminal-status-reconnecting"
        );
        assert_eq!(TerminalConnState::Disconnected.as_str(), "disconnected");
        assert_eq!(
            TerminalConnState::Disconnected.css_class(),
            "terminal-status-disconnected"
        );
        assert!(TerminalConnState::Disconnected.label().contains("Disconnected"));
    }
}
