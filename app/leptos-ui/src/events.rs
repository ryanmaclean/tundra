use leptos::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{CloseEvent, ErrorEvent, MessageEvent, WebSocket};

use crate::api;

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
    // Holds the active WebSocket so on_cleanup can close it.
    let ws_handle: Rc<Cell<Option<WebSocket>>> = Rc::new(Cell::new(None));

    // Start connection in a spawn_local
    let set_toasts_clone = set_toasts;
    let set_conn_state_clone = set_conn_state;
    let set_latest_event_clone = set_latest_event;
    let mounted_clone = mounted.clone();
    let ws_handle_clone = ws_handle.clone();

    spawn_local(async move {
        connect_ws(
            set_conn_state_clone,
            set_latest_event_clone,
            set_toasts_clone,
            set_unread_count,
            mounted_clone,
            ws_handle_clone,
        );
    });

    // Cleanup: close the WebSocket and mark unmounted so reconnect is skipped.
    // SendWrapper is needed because Rc is not Send+Sync, but WASM is single-threaded.
    let mounted_cleanup = send_wrapper::SendWrapper::new(mounted.clone());
    let ws_handle_cleanup = send_wrapper::SendWrapper::new(ws_handle.clone());
    on_cleanup(move || {
        mounted_cleanup.set(false);
        if let Some(ws) = (*ws_handle_cleanup).take() {
            ws.close().ok();
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

fn connect_ws(
    set_conn_state: WriteSignal<WsConnectionState>,
    set_latest_event: WriteSignal<Option<serde_json::Value>>,
    set_toasts: WriteSignal<Vec<Toast>>,
    set_unread_count: WriteSignal<u64>,
    mounted: Rc<Cell<bool>>,
    ws_handle: Rc<Cell<Option<WebSocket>>>,
) {
    // Don't connect if the component has already been unmounted.
    if !mounted.get() {
        return;
    }

    let url = api::events_ws_url();

    set_conn_state.set(WsConnectionState::Connecting);

    let ws = match WebSocket::new(&url) {
        Ok(ws) => ws,
        Err(_) => {
            set_conn_state.set(WsConnectionState::Disconnected);
            schedule_reconnect(
                set_conn_state,
                set_latest_event,
                set_toasts,
                set_unread_count,
                1,
                mounted,
                ws_handle,
            );
            return;
        }
    };

    // Store the WebSocket so on_cleanup can close it.
    ws_handle.set(Some(ws.clone()));

    // On open
    {
        let set_state = set_conn_state;
        let onopen = Closure::wrap(Box::new(move |_: JsValue| {
            set_state.set(WsConnectionState::Connected);
            web_sys::console::log_1(&"[events] WebSocket connected".into());
        }) as Box<dyn FnMut(JsValue)>);
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();
    }

    // On message
    {
        let set_event = set_latest_event;
        let set_toasts2 = set_toasts;
        let set_unread = set_unread_count;
        let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
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
                        let set_count = set_unread;
                        wasm_bindgen_futures::spawn_local(async move {
                            if let Ok(count) = crate::api::fetch_notification_count().await {
                                set_count.set(count.unread);
                            }
                        });
                    }
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();
    }

    // On error
    {
        let onerror = Closure::wrap(Box::new(move |_: ErrorEvent| {
            web_sys::console::warn_1(&"[events] WebSocket error".into());
        }) as Box<dyn FnMut(ErrorEvent)>);
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();
    }

    // On close -- trigger reconnect with backoff (only if still mounted)
    {
        let set_state = set_conn_state;
        let set_event = set_latest_event;
        let set_toasts3 = set_toasts;
        let set_unread = set_unread_count;
        let mounted_close = mounted.clone();
        let ws_handle_close = ws_handle.clone();
        let onclose = Closure::wrap(Box::new(move |_: CloseEvent| {
            set_state.set(WsConnectionState::Disconnected);
            if mounted_close.get() {
                web_sys::console::log_1(&"[events] WebSocket closed, will reconnect".into());
                schedule_reconnect(
                    set_state,
                    set_event,
                    set_toasts3,
                    set_unread,
                    1,
                    mounted_close.clone(),
                    ws_handle_close.clone(),
                );
            } else {
                web_sys::console::log_1(
                    &"[events] WebSocket closed (component unmounted, skipping reconnect)".into(),
                );
            }
        }) as Box<dyn FnMut(CloseEvent)>);
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();
    }
}

fn schedule_reconnect(
    set_conn_state: WriteSignal<WsConnectionState>,
    set_latest_event: WriteSignal<Option<serde_json::Value>>,
    set_toasts: WriteSignal<Vec<Toast>>,
    set_unread_count: WriteSignal<u64>,
    attempt: u32,
    mounted: Rc<Cell<bool>>,
    ws_handle: Rc<Cell<Option<WebSocket>>>,
) {
    // Don't schedule reconnect if the component has been unmounted.
    if !mounted.get() {
        return;
    }

    let delay_secs = std::cmp::min(2u32.pow(attempt.saturating_sub(1)), 16);
    set_conn_state.set(WsConnectionState::Reconnecting);

    spawn_local(async move {
        gloo_timers::future::TimeoutFuture::new(delay_secs * 1000).await;
        // Re-check mounted after the delay — the component may have unmounted while waiting.
        if !mounted.get() {
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
        );
    });
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
