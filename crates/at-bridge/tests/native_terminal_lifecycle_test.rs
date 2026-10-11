//! Actual HTTP/WebSocket -> shared native owner lifecycle; unrun source.
use std::{sync::Arc, time::Duration, io::{BufRead, Write}};
use at_bridge::{event_bus::EventBus, http_api::{api_router, ApiState}};
use at_session::pty_pool::PtyPool;
use futures_util::{FutureExt, SinkExt, StreamExt};
use serde_json::Value;

#[test]
#[ignore = "internal native child fixture; invoked only by explicit configured test program"]
fn native_echo_fixture() {
    println!("NATIVE_READY"); std::io::stdout().flush().unwrap();
    for line in std::io::stdin().lock().lines() {
        println!("REPLY:{}", line.unwrap()); std::io::stdout().flush().unwrap();
    }
}
fn request(url: &str) -> tokio_tungstenite::tungstenite::http::Request<()> {
    tokio_tungstenite::tungstenite::http::Request::builder().uri(url)
        .header("Host", "localhost").header("Origin", "http://localhost")
        .header("Connection", "Upgrade").header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", tokio_tungstenite::tungstenite::handshake::client::generate_key())
        .body(()).unwrap()
}
#[tokio::test]
#[ignore = "requires admitted remote native target and full API dependency graph"]
async fn native_http_capacity_reconnect_delete_shutdown() {
    let exe = std::env::current_exe().unwrap();
    let args = ["--exact", "native_echo_fixture", "--ignored", "--nocapture"].map(str::to_owned);
    let pool = Arc::new(PtyPool::new(1));
    let state = Arc::new(ApiState::with_pty_pool(EventBus::new(), pool.clone())
        .with_terminal_command(Some(exe.to_str().unwrap()), &args).with_relaxed_rate_limits());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = api_router(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap(); });
    let outcome = std::panic::AssertUnwindSafe(async {
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let (left, right) = tokio::join!(client.post(format!("{base}/api/terminals")).send(), client.post(format!("{base}/api/terminals")).send());
    let (left, right) = (left.unwrap(), right.unwrap());
    assert_eq!(usize::from(left.status() == 201) + usize::from(right.status() == 201), 1);
    let created: Value = if left.status() == 201 { left.json().await.unwrap() } else { right.json().await.unwrap() };
    let id = created["id"].as_str().unwrap();
    let ws_url = format!("ws://{addr}/ws/terminal/{id}");
    let (mut first, _) = tokio_tungstenite::connect_async(request(&ws_url)).await.unwrap();
    first.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::json!({"type":"input","data":"first\n"}).to_string().into())).await.unwrap();
    let (mut next, _) = tokio_tungstenite::connect_async(request(&ws_url)).await.unwrap();
    next.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::json!({"type":"input","data":"successor\n"}).to_string().into())).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(message) = next.next().await {
            if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = message {
                if text.contains("REPLY:successor") { return; }
            }
        }
        panic!("successor lost native output");
    }).await.unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let response = client.delete(format!("{base}/api/terminals/{id}")).send().await.unwrap();
            if response.status() == 200 { break; }
            assert_eq!(response.status(), 202);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    assert_eq!(pool.active_count(), 0);
    assert!(state.pty_handles.read().await.is_empty());
    let created: Value = client.post(format!("{base}/api/terminals")).send().await.unwrap().json().await.unwrap();
    assert!(created["id"].as_str().is_some());
    assert!(state.shutdown_terminals(Duration::from_secs(10)).await.is_empty());
    assert_eq!(pool.active_count(), 0);
    assert_ne!(client.post(format!("{base}/api/terminals")).send().await.unwrap().status(), 201);
    }).catch_unwind().await;
    // A failed assertion still enters the real asynchronous retirement barrier;
    // do not detach native fixtures merely because this test is failing.
    server.abort(); let _ = server.await;
    let pending = state.shutdown_terminals(Duration::from_secs(15)).await;
    if let Err(panic) = outcome { std::panic::resume_unwind(panic); }
    assert!(pending.is_empty(), "native fixture cleanup remained pending: {pending:?}");
}
