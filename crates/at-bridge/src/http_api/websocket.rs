use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::{extract::State, response::IntoResponse};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;

use crate::notifications::notification_from_event;
use crate::origin_validation::{get_default_allowed_origins, validate_websocket_origin};

use super::state::ApiState;

/// WebSocket GET /ws -- legacy real-time event streaming endpoint.
pub(crate) async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Validate Origin header to prevent cross-site WebSocket hijacking
    if let Err(status) = validate_websocket_origin(&headers, &get_default_allowed_origins()) {
        return status.into_response();
    }

    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

/// Internal handler that processes the upgraded WebSocket connection.
async fn handle_ws(mut socket: WebSocket, state: Arc<ApiState>) {
    let rx = state.event_bus.subscribe();
    while let Ok(msg) = rx.recv_async().await {
        let json = serde_json::to_string(&*msg).unwrap_or_default();
        if socket.send(Message::Text(json.into())).await.is_err() {
            break;
        }
    }
}

/// WebSocket GET /api/events/ws -- real-time event streaming with heartbeat and notification integration.
pub(crate) async fn events_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Validate Origin header to prevent cross-site WebSocket hijacking
    if let Err(status) = validate_websocket_origin(&headers, &get_default_allowed_origins()) {
        return status.into_response();
    }

    ws.on_upgrade(move |socket| handle_events_ws(socket, state))
}

/// Internal handler that processes the upgraded WebSocket connection with heartbeat support.
async fn handle_events_ws(socket: WebSocket, state: Arc<ApiState>) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let rx = state.event_bus.subscribe();
    let notification_store = state.notification_store.clone();

    // Heartbeat interval: 30 seconds
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(30));

    loop {
        tokio::select! {
            // Forward events from the bus to the WebSocket client
            result = rx.recv_async() => {
                match result {
                    Ok(msg) => {
                        // Wire event to notification store
                        if let Some((title, message, level, source, action_url)) = notification_from_event(&msg) {
                            let mut store = notification_store.write().await;
                            store.add_with_url(title, message, level, source, action_url);
                        }

                        let json = serde_json::to_string(&*msg).unwrap_or_default();
                        if ws_tx.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            // Send heartbeat ping every 30s
            _ = heartbeat.tick() => {
                let ping_msg = serde_json::json!({"type": "ping", "timestamp": chrono::Utc::now().to_rfc3339()});
                if ws_tx.send(Message::Text(ping_msg.to_string().into())).await.is_err() {
                    break;
                }
            }

            // Handle incoming messages from client (pong, close, etc.)
            incoming = ws_rx.next() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {} // Ignore other messages (pong, text commands, etc.)
                }
            }
        }
    }
}
