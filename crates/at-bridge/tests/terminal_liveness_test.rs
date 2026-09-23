//! Terminal WebSocket liveness: a quiet PTY must not close the connection,
//! but a client that stops answering heartbeats must.
//!
//! Regression for: the reader task wrapped PTY output in a 5-minute idle
//! timeout, so an idle prompt closed the socket and the reconnect-grace task
//! then killed the shell.

use std::sync::Arc;
use std::time::Duration;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use at_bridge::terminal::TerminalStatus;
use at_bridge::terminal_ws::TerminalWsSettings;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio_tungstenite::tungstenite::protocol::Message;
use uuid::Uuid;

fn ws_request(url: &str) -> tokio_tungstenite::tungstenite::http::Request<()> {
    tokio_tungstenite::tungstenite::http::Request::builder()
        .uri(url)
        .header("Host", "localhost")
        .header("Origin", "http://localhost")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .body(())
        .unwrap()
}

/// Server with fast heartbeats: Ping every 100 ms, dead after 500 ms silence.
async fn start_server() -> (String, Arc<ApiState>) {
    let pool = Arc::new(at_session::pty_pool::PtyPool::new(4));
    let mut state = ApiState::with_pty_pool(EventBus::new(), pool).with_relaxed_rate_limits();
    state.terminal_ws = TerminalWsSettings {
        heartbeat_interval: Duration::from_millis(100),
        liveness_timeout: Some(Duration::from_millis(500)),
    };
    let state = Arc::new(state);
    let router = api_router(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{addr}"), state)
}

async fn create_terminal(base: &str) -> Uuid {
    let resp = reqwest::Client::new()
        .post(format!("{base}/api/terminals"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let v: Value = resp.json().await.unwrap();
    v["id"].as_str().unwrap().parse().unwrap()
}

async fn status(state: &ApiState, id: Uuid) -> TerminalStatus {
    state
        .terminal_registry
        .read()
        .await
        .get(&id)
        .expect("terminal registered")
        .status
        .clone()
}

#[tokio::test]
async fn quiet_terminal_with_live_client_stays_connected() {
    let (base, state) = start_server().await;
    let id = create_terminal(&base).await;
    let ws_url = base.replace("http://", "ws://") + &format!("/ws/terminal/{id}");
    let (mut ws, _) = tokio_tungstenite::connect_async(ws_request(&ws_url))
        .await
        .unwrap();

    // Keep polling (tungstenite answers Pings with Pongs while we read) for
    // 4x the liveness timeout while the shell prints nothing.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(50), ws.next()).await {
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Ok(Some(Err(_))) => {
                panic!("server closed a healthy but quiet terminal connection")
            }
            _ => {}
        }
    }
    assert!(matches!(status(&state, id).await, TerminalStatus::Active));

    // The shell is still alive and reachable.
    ws.send(Message::Text(
        serde_json::json!({"type": "input", "data": "echo STILL_ALIVE_42\n"})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    let mut found = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline && !found {
        if let Ok(Some(Ok(Message::Text(t)))) =
            tokio::time::timeout(Duration::from_millis(200), ws.next()).await
        {
            found = t.contains("STILL_ALIVE_42");
        }
    }
    assert!(found, "expected echo output after quiet period");
}

#[tokio::test]
async fn unresponsive_client_is_disconnected() {
    let (base, state) = start_server().await;
    let id = create_terminal(&base).await;
    let ws_url = base.replace("http://", "ws://") + &format!("/ws/terminal/{id}");
    let (ws, _) = tokio_tungstenite::connect_async(ws_request(&ws_url))
        .await
        .unwrap();

    // Never read from the socket, so no Pongs are sent.
    let mut disconnected = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if matches!(
            status(&state, id).await,
            TerminalStatus::Disconnected { .. }
        ) {
            disconnected = true;
            break;
        }
    }
    assert!(
        disconnected,
        "server should drop a client that stops answering heartbeats"
    );
    drop(ws);
}

#[test]
fn liveness_settings_from_config() {
    assert_eq!(
        TerminalWsSettings::from_liveness_secs(0).liveness_timeout,
        None
    );
    // Clamped to at least two heartbeat intervals.
    assert_eq!(
        TerminalWsSettings::from_liveness_secs(5).liveness_timeout,
        Some(Duration::from_secs(60))
    );
    assert_eq!(
        TerminalWsSettings::from_liveness_secs(300).liveness_timeout,
        Some(Duration::from_secs(300))
    );
}
