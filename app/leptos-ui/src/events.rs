use leptos::prelude::*;
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
    pub level: String, // "info", "success", "warning", "error"
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
    ReadSignal<u64>,          // unread notification count
    WriteSignal<u64>,         // setter for unread count
) {
    let (conn_state, set_conn_state) = signal(WsConnectionState::Disconnected);
    let (latest_event, set_latest_event) = signal(None::<serde_json::Value>);
    let (toasts, set_toasts) = signal(Vec::<Toast>::new());
    let (unread_count, set_unread_count) = signal(0u64);

    // Start connection in a spawn_local
    let set_toasts_clone = set_toasts;
    let set_conn_state_clone = set_conn_state;
    let set_latest_event_clone = set_latest_event;

    spawn_local(async move {
        connect_ws(
            set_conn_state_clone,
            set_latest_event_clone,
            set_toasts_clone,
            set_unread_count,
        );
    });

    (conn_state, latest_event, toasts, set_toasts, unread_count, set_unread_count)
}

fn connect_ws(
    set_conn_state: WriteSignal<WsConnectionState>,
    set_latest_event: WriteSignal<Option<serde_json::Value>>,
    set_toasts: WriteSignal<Vec<Toast>>,
    set_unread_count: WriteSignal<u64>,
) {
    let url = api::events_ws_url();

    set_conn_state.set(WsConnectionState::Connecting);

    let ws = match WebSocket::new(&url) {
        Ok(ws) => ws,
        Err(_) => {
            set_conn_state.set(WsConnectionState::Disconnected);
            schedule_reconnect(set_conn_state, set_latest_event, set_toasts, set_unread_count, 1);
            return;
        }
    };

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

                    // Bump unread count for Event-type messages
                    if value.get("type").and_then(|t| t.as_str()) == Some("event") {
                        set_unread.update(|c| *c += 1);
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

    // On close -- trigger reconnect with backoff
    {
        let set_state = set_conn_state;
        let set_event = set_latest_event;
        let set_toasts3 = set_toasts;
        let set_unread = set_unread_count;
        let onclose = Closure::wrap(Box::new(move |_: CloseEvent| {
            set_state.set(WsConnectionState::Disconnected);
            web_sys::console::log_1(&"[events] WebSocket closed, will reconnect".into());
            schedule_reconnect(set_state, set_event, set_toasts3, set_unread, 1);
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
) {
    let delay_secs = std::cmp::min(2u32.pow(attempt.saturating_sub(1)), 16);
    set_conn_state.set(WsConnectionState::Reconnecting);

    spawn_local(async move {
        gloo_timers::future::TimeoutFuture::new(delay_secs * 1000).await;
        web_sys::console::log_1(
            &format!("[events] Reconnect attempt {} (delay {}s)", attempt, delay_secs).into(),
        );
        connect_ws(set_conn_state, set_latest_event, set_toasts, set_unread_count);
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

            Some(Toast {
                id: uuid::Uuid::new_v4().to_string(),
                title,
                message,
                level,
            })
        }
        _ => None,
    }
}
