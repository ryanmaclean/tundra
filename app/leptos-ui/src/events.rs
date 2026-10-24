use leptos::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{CloseEvent, ErrorEvent, MessageEvent, WebSocket};

use crate::api;
use crate::EnrollmentControl;

/// Represents a parsed event from the WebSocket stream.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WsEvent {
    #[serde(default, rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

/// Connection state for the event WebSocket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsConnectionState {
    Connecting,
    Connected,
    Disconnected,
    Reconnecting,
}

/// Notification toast that should be shown to the user.
#[derive(Debug, Clone)]
pub struct Toast {
    pub id: String,
    pub title: String,
    pub message: String,
    pub level: String,            // "info", "success", "warning", "error"
    pub duration_ms: Option<u64>, // Auto-dismiss duration in milliseconds (None = no auto-dismiss)
    pub auto_dismiss: bool,       // Whether this toast should auto-dismiss
}

/// Own every JS callback for one event socket. Reconnect replaces the whole
/// binding after unbinding; no Closure::forget grows per attempt.
struct BoundEventSocket {
    ws: WebSocket,
    _on_open: Closure<dyn FnMut(JsValue)>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(ErrorEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
}

impl BoundEventSocket {
    fn shutdown(self) {
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onerror(None);
        self.ws.set_onclose(None);
        let _ = self.ws.close();
    }
}

/// Start the WebSocket event subscription. Returns signals for connection state,
/// latest event, and toast notifications.
///
/// Features:
/// - Auto-reconnect with exponential backoff (1s, 2s, 4s, 8s, 16s max)
/// - Parses incoming JSON events
/// - Fires toast notifications for important events
/// - Tracks connection state
pub fn use_event_stream() -> (
    ReadSignal<WsConnectionState>,
    ReadSignal<Option<serde_json::Value>>,
    ReadSignal<Vec<Toast>>,
    WriteSignal<Vec<Toast>>,
    ReadSignal<u64>,  // unread notification count
    WriteSignal<u64>, // setter for unread count
) {
    let (conn_state, set_conn_state) = signal(WsConnectionState::Disconnected);
    let (latest_event, set_latest_event) = signal(None::<serde_json::Value>);
    let (toasts, set_toasts) = signal(Vec::<Toast>::new());
    let (unread_count, set_unread_count) = signal(0u64);

    // Track whether the component is still mounted so we skip reconnect after unmount.
    let mounted = Rc::new(Cell::new(true));
    // Owns one socket and its four callbacks, including while reconnect waits.
    let ws_handle: Rc<RefCell<Option<BoundEventSocket>>> = Rc::new(RefCell::new(None));

    // Start connection in a spawn_local
    let set_toasts_clone = set_toasts;
    let set_conn_state_clone = set_conn_state;
    let set_latest_event_clone = set_latest_event;
    let mounted_clone = mounted.clone();
    let ws_handle_clone = ws_handle.clone();

    // Consecutive failed connection attempts; drives exponential backoff and
    // is reset once a connection opens.
    let failures: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let generation = Rc::new(Cell::new(0u64));
    // Increasing across reconnects so overlapping refresh responses cannot
    // overwrite a newer count from the current event stream.
    let refresh_token = Rc::new(Cell::new(0u64));
    let enrollment = use_context::<EnrollmentControl>();
    let generation_start = generation.clone();
    let refresh_start = refresh_token.clone();

    spawn_local(async move {
        connect_ws(
            set_conn_state_clone,
            set_latest_event_clone,
            set_toasts_clone,
            set_unread_count,
            mounted_clone,
            ws_handle_clone,
            failures,
            generation_start,
            refresh_start,
            enrollment,
        )
        .await;
    });

    // Cleanup: close the WebSocket and mark unmounted so reconnect is skipped.
    // SendWrapper is needed because Rc is not Send+Sync, but WASM is single-threaded.
    let mounted_cleanup = send_wrapper::SendWrapper::new(mounted.clone());
    let ws_handle_cleanup = send_wrapper::SendWrapper::new(ws_handle.clone());
    let generation_cleanup = send_wrapper::SendWrapper::new(generation);
    let refresh_cleanup = send_wrapper::SendWrapper::new(refresh_token);
    on_cleanup(move || {
        mounted_cleanup.set(false);
        generation_cleanup.set(generation_cleanup.get().wrapping_add(1));
        refresh_cleanup.set(refresh_cleanup.get().wrapping_add(1));
        if let Some(bound) = ws_handle_cleanup.borrow_mut().take() {
            bound.shutdown();
        }
    });

    (
        conn_state,
        latest_event,
        toasts,
        set_toasts,
        unread_count,
        set_unread_count,
    )
}

async fn connect_ws(
    set_conn_state: WriteSignal<WsConnectionState>,
    set_latest_event: WriteSignal<Option<serde_json::Value>>,
    set_toasts: WriteSignal<Vec<Toast>>,
    set_unread_count: WriteSignal<u64>,
    mounted: Rc<Cell<bool>>,
    ws_handle: Rc<RefCell<Option<BoundEventSocket>>>,
    failures: Rc<Cell<u32>>,
    generation: Rc<Cell<u64>>,
    refresh_token: Rc<Cell<u64>>,
    enrollment: Option<EnrollmentControl>,
) {
    // Don't connect if the component has already been unmounted.
    if !mounted.get() {
        return;
    }
    if let Some(bound) = ws_handle.borrow_mut().take() {
        bound.shutdown();
    }

    let attempt = generation.get().wrapping_add(1);
    generation.set(attempt);
    set_conn_state.set(WsConnectionState::Connecting);
    // Keep the initial bell count available even if the WebSocket handshake
    // fails; this shares the same ordering gate as event-triggered refreshes.
    refresh_notification_count(
        set_unread_count,
        mounted.clone(),
        generation.clone(),
        attempt,
        refresh_token.clone(),
    );
    let ticket = api::issue_ws_ticket("/api/events/ws").await;
    if !mounted.get() || generation.get() != attempt {
        return;
    }
    if ticket == Err(api::WsTicketError::Unauthorized) {
        api::clear_browser_key();
        if let Some(control) = enrollment {
            control.0.set(false);
        }
        return;
    }
    let ws = match ticket.and_then(|ticket| api::open_ws("/api/events/ws", ticket.as_deref())) {
        Ok(ws) => ws,
        Err(_) => {
            set_conn_state.set(WsConnectionState::Disconnected);
            failures.set(failures.get().saturating_add(1));
            schedule_reconnect(
                set_conn_state,
                set_latest_event,
                set_toasts,
                set_unread_count,
                mounted,
                ws_handle,
                failures,
                generation,
                refresh_token,
                enrollment,
            );
            return;
        }
    };

    // On open
    let open_callback = {
        let set_state = set_conn_state;
        let failures_open = failures.clone();
        let generation_open = generation.clone();
        let onopen = Closure::wrap(Box::new(move |_: JsValue| {
            if generation_open.get() != attempt {
                return;
            }
            failures_open.set(0);
            set_state.set(WsConnectionState::Connected);
            web_sys::console::log_1(&"[events] WebSocket connected".into());
        }) as Box<dyn FnMut(JsValue)>);
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen
    };

    // On message
    let message_callback = {
        let set_event = set_latest_event;
        let set_toasts2 = set_toasts;
        let set_unread = set_unread_count;
        let generation_message = generation.clone();
        let mounted_message = mounted.clone();
        let refresh_message = refresh_token.clone();
        let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
            if !mounted_message.get() || generation_message.get() != attempt {
                return;
            }
            if let Some(text) = e.data().as_string() {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                    // Skip ping messages
                    if value.get("type").and_then(|t| t.as_str()) == Some("ping") {
                        return;
                    }
                    set_event.set(Some(value.clone()));

                    // Generate toast for important events
                    if let Some(toast) = event_to_toast(&value) {
                        set_toasts2.update(|list| {
                            list.push(toast);
                            // Keep only last 10 toasts
                            if list.len() > 10 {
                                list.drain(0..list.len() - 10);
                            }
                        });
                    }

                    // Refresh unread count from the API when an event arrives,
                    // instead of blindly incrementing (which caused 99+ drift).
                    if value.get("type").and_then(|t| t.as_str()) == Some("event") {
                        refresh_notification_count(
                            set_unread,
                            mounted_message.clone(),
                            generation_message.clone(),
                            attempt,
                            refresh_message.clone(),
                        );
                    }
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage
    };

    // On error
    let error_callback = {
        let onerror = Closure::wrap(Box::new(move |_: ErrorEvent| {
            web_sys::console::warn_1(&"[events] WebSocket error".into());
        }) as Box<dyn FnMut(ErrorEvent)>);
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror
    };

    // On close -- trigger reconnect with backoff (only if still mounted)
    let close_callback = {
        let set_state = set_conn_state;
        let set_event = set_latest_event;
        let set_toasts3 = set_toasts;
        let set_unread = set_unread_count;
        let mounted_close = mounted.clone();
        let ws_handle_close = Rc::downgrade(&ws_handle);
        let failures_close = failures.clone();
        let generation_close = generation.clone();
        let onclose = Closure::wrap(Box::new(move |_: CloseEvent| {
            if generation_close.get() != attempt {
                return;
            }
            set_state.set(WsConnectionState::Disconnected);
            if mounted_close.get() {
                web_sys::console::log_1(&"[events] WebSocket closed, will reconnect".into());
                // A close before onopen (e.g. 401/403 on upgrade) counts as a
                // failure too, so the delay grows instead of retrying every 1s.
                failures_close.set(failures_close.get().saturating_add(1));
                let Some(ws_handle_close) = ws_handle_close.upgrade() else { return; };
                schedule_reconnect(
                    set_state,
                    set_event,
                    set_toasts3,
                    set_unread,
                    mounted_close.clone(),
                    ws_handle_close.clone(),
                    failures_close.clone(),
                    generation_close.clone(),
                    refresh_token.clone(),
                    enrollment,
                );
            } else {
                web_sys::console::log_1(
                    &"[events] WebSocket closed (component unmounted, skipping reconnect)".into(),
                );
            }
        }) as Box<dyn FnMut(CloseEvent)>);
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose
    };
    *ws_handle.borrow_mut() = Some(BoundEventSocket {
        ws,
        _on_open: open_callback,
        _on_message: message_callback,
        _on_error: error_callback,
        _on_close: close_callback,
    });
}

/// One ordering gate for initial and event-triggered count requests. A later
/// request wins even if its response arrives before an older response.
fn refresh_notification_count(
    set_count: WriteSignal<u64>,
    mounted: Rc<Cell<bool>>,
    generation: Rc<Cell<u64>>,
    expected_generation: u64,
    latest_token: Rc<Cell<u64>>,
) {
    let token = latest_token.get().wrapping_add(1);
    latest_token.set(token);
    spawn_local(async move {
        if let Ok(count) = api::fetch_notification_count().await {
            if refresh_result_is_current(
                mounted.get(), generation.get(), expected_generation,
                latest_token.get(), token,
            ) {
                set_count.set(count.unread);
            }
        }
    });
}

fn refresh_result_is_current(
    mounted: bool,
    generation: u64,
    expected_generation: u64,
    latest_token: u64,
    token: u64,
) -> bool {
    mounted && generation == expected_generation && latest_token == token
}

fn schedule_reconnect(
    set_conn_state: WriteSignal<WsConnectionState>,
    set_latest_event: WriteSignal<Option<serde_json::Value>>,
    set_toasts: WriteSignal<Vec<Toast>>,
    set_unread_count: WriteSignal<u64>,
    mounted: Rc<Cell<bool>>,
    ws_handle: Rc<RefCell<Option<BoundEventSocket>>>,
    failures: Rc<Cell<u32>>,
    generation: Rc<Cell<u64>>,
    refresh_token: Rc<Cell<u64>>,
    enrollment: Option<EnrollmentControl>,
) {
    // Don't schedule reconnect if the component has been unmounted.
    if !mounted.get() {
        return;
    }

    let attempt = failures.get().max(1);
    let delay_secs = reconnect_delay_secs(attempt);
    let scheduled_generation = generation.get();
    set_conn_state.set(WsConnectionState::Reconnecting);

    spawn_local(async move {
        gloo_timers::future::TimeoutFuture::new(delay_secs * 1000).await;
        // Re-check mounted after the delay — the component may have unmounted while waiting.
        if !mounted.get() || generation.get() != scheduled_generation {
            web_sys::console::log_1(&"[events] Skipping reconnect, component unmounted".into());
            return;
        }
        web_sys::console::log_1(
            &format!(
                "[events] Reconnect attempt {} (delay {}s)",
                attempt, delay_secs
            )
            .into(),
        );
        connect_ws(
            set_conn_state,
            set_latest_event,
            set_toasts,
            set_unread_count,
            mounted,
            ws_handle,
            failures,
            generation,
            refresh_token,
            enrollment,
        )
        .await;
    });
}

/// Exponential backoff: 1, 2, 4, 8, 16, 16, ... seconds for attempt 1, 2, ...
pub fn reconnect_delay_secs(attempt: u32) -> u32 {
    2u32.saturating_pow(attempt.saturating_sub(1)).min(16)
}

/// Convert a raw JSON event into a toast notification if it's important enough.
fn event_to_toast(value: &serde_json::Value) -> Option<Toast> {
    let msg_type = value.get("type")?.as_str()?;
    match msg_type {
        "event" => {
            let payload = value.get("payload")?;
            let event_type = payload.get("event_type")?.as_str()?;
            let message = payload.get("message")?.as_str().unwrap_or("").to_string();

            let (title, level) = match event_type {
                "bead_state_change" => ("Bead Updated".to_string(), "info".to_string()),
                "agent_spawned" => ("Agent Spawned".to_string(), "info".to_string()),
                "agent_stopped" => ("Agent Stopped".to_string(), "warning".to_string()),
                "agent_crashed" => ("Agent Crashed".to_string(), "error".to_string()),
                "task_completed" => ("Task Completed".to_string(), "success".to_string()),
                _ => return None,
            };

            // Set duration and auto_dismiss based on toast level
            let (duration_ms, auto_dismiss) = match level.as_str() {
                "info" | "success" => (Some(5000), true),
                "warning" => (Some(8000), true),
                "error" => (None, false),
                _ => (Some(5000), true), // Default to 5s for unknown levels
            };

            Some(Toast {
                id: uuid::Uuid::new_v4().to_string(),
                title,
                message,
                level,
                duration_ms,
                auto_dismiss,
            })
        }
        _ => None,
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod backoff_tests {
    use super::{reconnect_delay_secs, refresh_result_is_current};

    #[test]
    fn count_refresh_rejects_unmount_reconnect_and_older_response() {
        assert!(refresh_result_is_current(true, 7, 7, 2, 2));
        assert!(!refresh_result_is_current(false, 7, 7, 2, 2));
        assert!(!refresh_result_is_current(true, 8, 7, 2, 2));
        assert!(!refresh_result_is_current(true, 7, 7, 3, 2));
    }

    #[test]
    fn reconnect_backoff_grows_and_caps() {
        let delays: Vec<u32> = (1..=7).map(reconnect_delay_secs).collect();
        assert_eq!(delays, vec![1, 2, 4, 8, 16, 16, 16]);
        assert_eq!(reconnect_delay_secs(u32::MAX), 16);
    }
}
