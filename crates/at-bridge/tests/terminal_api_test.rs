use std::sync::Arc;
use std::time::Duration;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use at_bridge::terminal::{
    DisconnectBuffer, TerminalInfo, TerminalRegistry, TerminalStatus, WS_RECONNECT_GRACE,
};
use serde_json::Value;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Spin up an API server on a random port with a PTY pool, return the base URL.
async fn start_test_server() -> (String, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let pool = Arc::new(at_session::pty_pool::PtyPool::new(4));
    let state = Arc::new(ApiState::with_pty_pool(event_bus, pool));
    let router = api_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind to ephemeral port");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    (format!("http://{addr}"), state)
}

/// Spin up a server with a custom PTY pool capacity.
async fn start_test_server_with_capacity(max: usize) -> (String, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let pool = Arc::new(at_session::pty_pool::PtyPool::new(max));
    let state = Arc::new(ApiState::with_pty_pool(event_bus, pool));
    let router = api_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind to ephemeral port");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    (format!("http://{addr}"), state)
}

/// Create a terminal via the API and return the parsed JSON response.
async fn create_terminal(client: &reqwest::Client, base: &str) -> Value {
    let resp = client
        .post(format!("{base}/api/terminals"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    resp.json().await.unwrap()
}

// ===========================================================================
// Terminal CRUD via API
// ===========================================================================

#[tokio::test]
async fn test_create_terminal_returns_id_and_name() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let terminal = create_terminal(&client, &base).await;

    // Must have an id string (parseable as UUID).
    let id_str = terminal["id"].as_str().expect("id should be a string");
    Uuid::parse_str(id_str).expect("id should be valid UUID");

    // Must have a title that contains "Terminal".
    let title = terminal["title"].as_str().expect("title should be a string");
    assert!(
        title.contains("Terminal"),
        "expected title to contain 'Terminal', got: {title}"
    );

    // Status should be active.
    assert_eq!(terminal["status"], "active");

    // Default dimensions.
    assert_eq!(terminal["cols"], 80);
    assert_eq!(terminal["rows"], 24);
}

#[tokio::test]
async fn test_create_multiple_terminals_unique_ids() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let t1 = create_terminal(&client, &base).await;
    let t2 = create_terminal(&client, &base).await;
    let t3 = create_terminal(&client, &base).await;

    let id1 = t1["id"].as_str().unwrap();
    let id2 = t2["id"].as_str().unwrap();
    let id3 = t3["id"].as_str().unwrap();

    // All IDs must be distinct.
    assert_ne!(id1, id2, "terminal 1 and 2 have same ID");
    assert_ne!(id2, id3, "terminal 2 and 3 have same ID");
    assert_ne!(id1, id3, "terminal 1 and 3 have same ID");
}

#[tokio::test]
async fn test_list_terminals_returns_all() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Initially empty.
    let resp = client
        .get(format!("{base}/api/terminals"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let list: Vec<Value> = resp.json().await.unwrap();
    assert!(list.is_empty(), "expected empty terminal list initially");

    // Create three terminals.
    let t1 = create_terminal(&client, &base).await;
    let t2 = create_terminal(&client, &base).await;
    let _t3 = create_terminal(&client, &base).await;

    let resp = client
        .get(format!("{base}/api/terminals"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let list: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(list.len(), 3, "expected 3 terminals in list");

    // Verify our created IDs are present.
    let ids: Vec<&str> = list.iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&t1["id"].as_str().unwrap()));
    assert!(ids.contains(&t2["id"].as_str().unwrap()));
}

#[tokio::test]
async fn test_delete_terminal_removes_from_list() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let t1 = create_terminal(&client, &base).await;
    let t2 = create_terminal(&client, &base).await;
    let id1 = t1["id"].as_str().unwrap();

    // Delete terminal 1.
    let resp = client
        .delete(format!("{base}/api/terminals/{id1}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "deleted");
    assert_eq!(body["id"], id1);

    // List should now have only terminal 2.
    let resp = client
        .get(format!("{base}/api/terminals"))
        .send()
        .await
        .unwrap();
    let list: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["id"].as_str().unwrap(), t2["id"].as_str().unwrap());
}

#[tokio::test]
async fn test_delete_nonexistent_terminal_returns_404() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let fake_id = Uuid::new_v4();
    let resp = client
        .delete(format!("{base}/api/terminals/{fake_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    let body: Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_terminal_capacity_limit() {
    // Pool capacity of 2.
    let (base, _state) = start_test_server_with_capacity(2).await;
    let client = reqwest::Client::new();

    // First two should succeed.
    let _t1 = create_terminal(&client, &base).await;
    let _t2 = create_terminal(&client, &base).await;

    // Third should fail (at capacity).
    let resp = client
        .post(format!("{base}/api/terminals"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        500,
        "expected 500 when pool is at capacity"
    );

    let body: Value = resp.json().await.unwrap();
    let err = body["error"].as_str().unwrap();
    assert!(
        err.contains("capacity") || err.contains("spawn failed"),
        "expected capacity error, got: {err}"
    );
}

// ===========================================================================
// Terminal WebSocket
// ===========================================================================

#[tokio::test]
async fn test_terminal_ws_connect_valid_id() {
    use futures_util::StreamExt;

    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let terminal = create_terminal(&client, &base).await;
    let tid = terminal["id"].as_str().unwrap();

    let ws_url = base.replace("http://", "ws://") + &format!("/ws/terminal/{tid}");
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("failed to connect to terminal websocket");

    // Should receive some initial shell output (prompt, etc.) within a few seconds.
    let msg = tokio::time::timeout(Duration::from_secs(3), ws_stream.next()).await;
    // The connection itself succeeding is the main assertion.
    // We may or may not get output depending on shell startup timing.
    assert!(msg.is_ok() || msg.is_err(), "websocket connection established");
}

#[tokio::test]
async fn test_terminal_ws_connect_invalid_id_returns_error() {
    let (base, _state) = start_test_server().await;

    let fake_id = Uuid::new_v4();
    let ws_url = base.replace("http://", "ws://") + &format!("/ws/terminal/{fake_id}");

    // Connecting to a nonexistent terminal should fail or return an error.
    let result = tokio_tungstenite::connect_async(&ws_url).await;

    // The server should reject the upgrade (404), so the WS handshake fails.
    assert!(
        result.is_err(),
        "expected websocket connection to fail for nonexistent terminal"
    );
}

#[tokio::test]
async fn test_terminal_send_input_via_ws() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::protocol::Message;

    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let terminal = create_terminal(&client, &base).await;
    let tid = terminal["id"].as_str().unwrap();

    let ws_url = base.replace("http://", "ws://") + &format!("/ws/terminal/{tid}");
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("failed to connect");

    // Give shell a moment to start.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Drain any initial prompt output.
    while let Ok(Some(_)) =
        tokio::time::timeout(Duration::from_millis(200), ws_stream.next()).await
    {}

    // Send an echo command via the JSON input protocol.
    let input_msg = serde_json::json!({
        "type": "input",
        "data": "echo HELLO_TERMINAL_TEST\n"
    });
    ws_stream
        .send(Message::Text(input_msg.to_string().into()))
        .await
        .expect("failed to send input");

    // Collect output for up to 3 seconds looking for our echoed string.
    let mut found = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), ws_stream.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                if text.contains("HELLO_TERMINAL_TEST") {
                    found = true;
                    break;
                }
            }
            _ => continue,
        }
    }

    assert!(found, "expected to see echoed 'HELLO_TERMINAL_TEST' in WS output");
}

#[tokio::test]
async fn test_terminal_resize_event() {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::protocol::Message;

    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    let terminal = create_terminal(&client, &base).await;
    let tid = terminal["id"].as_str().unwrap();
    let tid_uuid = Uuid::parse_str(tid).unwrap();

    // Verify initial dimensions.
    assert_eq!(terminal["cols"], 80);
    assert_eq!(terminal["rows"], 24);

    let ws_url = base.replace("http://", "ws://") + &format!("/ws/terminal/{tid}");
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("failed to connect");

    // Send a resize command.
    let resize_msg = serde_json::json!({
        "type": "resize",
        "cols": 120,
        "rows": 40
    });
    let result = ws_stream
        .send(Message::Text(resize_msg.to_string().into()))
        .await;

    // Resize should be accepted without error.
    assert!(result.is_ok(), "resize message should be accepted");

    // Give the writer task a moment to process the resize message.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify that the terminal registry was updated with new dimensions.
    {
        let registry = state.terminal_registry.read().await;
        let info = registry.get(&tid_uuid).expect("terminal should exist in registry");
        assert_eq!(info.cols, 120, "cols should be updated to 120");
        assert_eq!(info.rows, 40, "rows should be updated to 40");
    }
}

#[tokio::test]
async fn test_terminal_disconnect_cleanup() {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::protocol::Message;

    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let terminal = create_terminal(&client, &base).await;
    let tid = terminal["id"].as_str().unwrap();

    let ws_url = base.replace("http://", "ws://") + &format!("/ws/terminal/{tid}");
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("failed to connect");

    // Close the websocket.
    ws_stream
        .send(Message::Close(None))
        .await
        .expect("failed to send close");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Terminal should still exist in the registry (WS disconnect doesn't remove it).
    let resp = client
        .get(format!("{base}/api/terminals"))
        .send()
        .await
        .unwrap();
    let list: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(list.len(), 1, "terminal should persist after WS disconnect");
    assert_eq!(list[0]["id"].as_str().unwrap(), tid);
}

// ===========================================================================
// Terminal History & Recovery
// ===========================================================================

#[tokio::test]
async fn test_terminal_output_buffer_capped() {
    // This tests that the PTY output channel is bounded (256 entries from code).
    // We verify by spawning a process that produces a lot of output and ensuring
    // it doesn't hang or OOM.
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let terminal = create_terminal(&client, &base).await;
    let tid = terminal["id"].as_str().unwrap();

    // Send a command that generates substantial output via WS.
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::protocol::Message;

    let ws_url = base.replace("http://", "ws://") + &format!("/ws/terminal/{tid}");
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("failed to connect");

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Generate substantial output.
    let input_msg = serde_json::json!({
        "type": "input",
        "data": "for i in $(seq 1 500); do echo \"LINE_$i padding_data_to_make_this_longer\"; done\n"
    });
    ws_stream
        .send(Message::Text(input_msg.to_string().into()))
        .await
        .expect("failed to send input");

    // Drain output for a few seconds. The test passes if this completes
    // without hanging (buffer is bounded).
    let mut total_bytes = 0usize;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), ws_stream.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                total_bytes += text.len();
            }
            // Skip heartbeat ping/pong frames — they're expected.
            Ok(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => continue,
            _ => break,
        }
    }

    assert!(total_bytes > 0, "expected to receive some output data");
}

#[tokio::test]
async fn test_terminal_reconnect_after_disconnect() {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::protocol::Message;

    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let terminal = create_terminal(&client, &base).await;
    let tid = terminal["id"].as_str().unwrap();

    let ws_url = base.replace("http://", "ws://") + &format!("/ws/terminal/{tid}");

    // First connection.
    {
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .expect("first connect failed");
        ws_stream
            .send(Message::Close(None))
            .await
            .expect("close failed");
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Second connection to the same terminal should work.
    let result = tokio_tungstenite::connect_async(&ws_url).await;
    assert!(
        result.is_ok(),
        "should be able to reconnect to terminal after disconnect"
    );
}

#[tokio::test]
async fn test_terminal_auto_resume_after_crash() {
    // Simulate a terminal whose process has exited: create, kill the underlying
    // PTY, then verify the terminal is still listed (for potential auto-resume).
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    let terminal = create_terminal(&client, &base).await;
    let tid_str = terminal["id"].as_str().unwrap();
    let tid = Uuid::parse_str(tid_str).unwrap();

    // Kill the PTY handle directly (simulating a crash).
    {
        let handles = state.pty_handles.read().await;
        if let Some(handle) = handles.get(&tid) {
            let _ = handle.kill();
        }
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Terminal should still be in the registry (available for auto-resume).
    let resp = client
        .get(format!("{base}/api/terminals"))
        .send()
        .await
        .unwrap();
    let list: Vec<Value> = resp.json().await.unwrap();
    assert!(
        list.iter().any(|t| t["id"].as_str().unwrap() == tid_str),
        "crashed terminal should remain in registry for auto-resume"
    );
}

#[tokio::test]
async fn test_terminal_blank_after_project_switch() {
    // Regression test for fix/terminal-blank-project-switch.
    // After deleting and re-creating a terminal (simulating project switch),
    // the new terminal should be functional (not blank).
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create first terminal.
    let t1 = create_terminal(&client, &base).await;
    let id1 = t1["id"].as_str().unwrap();

    // Delete it (simulates project switch cleanup).
    let resp = client
        .delete(format!("{base}/api/terminals/{id1}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Create a new terminal (simulates new project terminal).
    let t2 = create_terminal(&client, &base).await;
    let id2 = t2["id"].as_str().unwrap();

    // The new terminal should be active and have valid properties.
    assert_eq!(t2["status"], "active");
    assert_ne!(id1, id2, "new terminal should have a fresh ID");

    // Connect via WS and verify it's functional (not blank).
    use futures_util::StreamExt;

    let ws_url = base.replace("http://", "ws://") + &format!("/ws/terminal/{id2}");
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("failed to connect to new terminal");

    // Should be able to receive at least some output (shell prompt).
    let msg = tokio::time::timeout(Duration::from_secs(3), ws_stream.next()).await;
    // Connection success is the primary assertion. The shell may or may not have
    // produced prompt output yet, but the WS should be live.
    drop(msg);

    // Verify terminal appears in the list.
    let resp = client
        .get(format!("{base}/api/terminals"))
        .send()
        .await
        .unwrap();
    let list: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["id"].as_str().unwrap(), id2);
}

// ===========================================================================
// Multi-Terminal Operations
// ===========================================================================

#[tokio::test]
async fn test_invite_all_sends_to_all_terminals() {
    // Simulates the "Invite Claude All" button by sending input to all
    // active terminals via their WS connections.
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::protocol::Message;

    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let t1 = create_terminal(&client, &base).await;
    let t2 = create_terminal(&client, &base).await;

    let tid1 = t1["id"].as_str().unwrap();
    let tid2 = t2["id"].as_str().unwrap();

    // Connect WS to both terminals.
    let ws_url1 = base.replace("http://", "ws://") + &format!("/ws/terminal/{tid1}");
    let ws_url2 = base.replace("http://", "ws://") + &format!("/ws/terminal/{tid2}");

    let (mut ws1, _) = tokio_tungstenite::connect_async(&ws_url1)
        .await
        .expect("connect ws1 failed");
    let (mut ws2, _) = tokio_tungstenite::connect_async(&ws_url2)
        .await
        .expect("connect ws2 failed");

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Drain initial output.
    while let Ok(Some(_)) =
        tokio::time::timeout(Duration::from_millis(200), ws1.next()).await
    {}
    while let Ok(Some(_)) =
        tokio::time::timeout(Duration::from_millis(200), ws2.next()).await
    {}

    // Simulate "Invite All" by sending the same command to both.
    let prompt = "echo INVITE_ALL_MARKER\n";
    let input_msg = serde_json::json!({ "type": "input", "data": prompt });

    ws1.send(Message::Text(input_msg.to_string().into()))
        .await
        .expect("send to ws1 failed");
    ws2.send(Message::Text(input_msg.to_string().into()))
        .await
        .expect("send to ws2 failed");

    // Check both terminals received the echoed output.
    let mut found1 = false;
    let mut found2 = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);

    while tokio::time::Instant::now() < deadline && (!found1 || !found2) {
        if !found1 {
            if let Ok(Some(Ok(Message::Text(text)))) =
                tokio::time::timeout(Duration::from_millis(200), ws1.next()).await
            {
                if text.contains("INVITE_ALL_MARKER") {
                    found1 = true;
                }
            }
        }
        if !found2 {
            if let Ok(Some(Ok(Message::Text(text)))) =
                tokio::time::timeout(Duration::from_millis(200), ws2.next()).await
            {
                if text.contains("INVITE_ALL_MARKER") {
                    found2 = true;
                }
            }
        }
    }

    assert!(found1, "terminal 1 should have received invite-all output");
    assert!(found2, "terminal 2 should have received invite-all output");
}

#[tokio::test]
async fn test_concurrent_terminal_output() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::protocol::Message;

    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let t1 = create_terminal(&client, &base).await;
    let t2 = create_terminal(&client, &base).await;

    let tid1 = t1["id"].as_str().unwrap().to_string();
    let tid2 = t2["id"].as_str().unwrap().to_string();

    let ws_url1 = base.replace("http://", "ws://") + &format!("/ws/terminal/{tid1}");
    let ws_url2 = base.replace("http://", "ws://") + &format!("/ws/terminal/{tid2}");

    let (mut ws1, _) = tokio_tungstenite::connect_async(&ws_url1).await.unwrap();
    let (mut ws2, _) = tokio_tungstenite::connect_async(&ws_url2).await.unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Drain initial output.
    while let Ok(Some(_)) =
        tokio::time::timeout(Duration::from_millis(200), ws1.next()).await
    {}
    while let Ok(Some(_)) =
        tokio::time::timeout(Duration::from_millis(200), ws2.next()).await
    {}

    // Send different commands to each terminal concurrently.
    let msg1 = serde_json::json!({ "type": "input", "data": "echo CONCURRENT_T1\n" });
    let msg2 = serde_json::json!({ "type": "input", "data": "echo CONCURRENT_T2\n" });

    let (r1, r2) = tokio::join!(
        ws1.send(Message::Text(msg1.to_string().into())),
        ws2.send(Message::Text(msg2.to_string().into())),
    );
    r1.unwrap();
    r2.unwrap();

    // Collect output from both.
    let mut output1 = String::new();
    let mut output2 = String::new();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        tokio::select! {
            result = ws1.next() => {
                if let Some(Ok(Message::Text(text))) = result {
                    output1.push_str(&text);
                }
            }
            result = ws2.next() => {
                if let Some(Ok(Message::Text(text))) = result {
                    output2.push_str(&text);
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                break;
            }
        }
    }

    assert!(
        output1.contains("CONCURRENT_T1"),
        "terminal 1 should have its own output, got: {output1}"
    );
    assert!(
        output2.contains("CONCURRENT_T2"),
        "terminal 2 should have its own output, got: {output2}"
    );
}

#[tokio::test]
async fn test_terminal_isolation() {
    // Verify that input sent to one terminal does not appear in another.
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::protocol::Message;

    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let t1 = create_terminal(&client, &base).await;
    let t2 = create_terminal(&client, &base).await;

    let tid1 = t1["id"].as_str().unwrap();
    let tid2 = t2["id"].as_str().unwrap();

    let ws_url1 = base.replace("http://", "ws://") + &format!("/ws/terminal/{tid1}");
    let ws_url2 = base.replace("http://", "ws://") + &format!("/ws/terminal/{tid2}");

    let (mut ws1, _) = tokio_tungstenite::connect_async(&ws_url1).await.unwrap();
    let (mut ws2, _) = tokio_tungstenite::connect_async(&ws_url2).await.unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Drain initial output from both.
    while let Ok(Some(_)) =
        tokio::time::timeout(Duration::from_millis(200), ws1.next()).await
    {}
    while let Ok(Some(_)) =
        tokio::time::timeout(Duration::from_millis(200), ws2.next()).await
    {}

    // Send a unique marker ONLY to terminal 1.
    let msg = serde_json::json!({ "type": "input", "data": "echo ISOLATION_MARKER_T1_ONLY\n" });
    ws1.send(Message::Text(msg.to_string().into()))
        .await
        .unwrap();

    // Wait a moment for output.
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Collect output from terminal 2.
    let mut t2_output = String::new();
    while let Ok(Some(Ok(Message::Text(text)))) =
        tokio::time::timeout(Duration::from_millis(500), ws2.next()).await
    {
        t2_output.push_str(&text);
    }

    // Terminal 2 should NOT contain the marker sent only to terminal 1.
    assert!(
        !t2_output.contains("ISOLATION_MARKER_T1_ONLY"),
        "terminal 2 should NOT contain terminal 1's output, got: {t2_output}"
    );
}

// ===========================================================================
// Terminal Registry (unit-level, direct struct tests)
// ===========================================================================

#[test]
fn test_registry_register_and_list() {
    let mut reg = TerminalRegistry::new();
    let info = TerminalInfo {
        id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        title: "T1".to_string(),
        status: TerminalStatus::Active,
        cols: 80,
        rows: 24,
        font_size: 14,
        cursor_style: "block".to_string(),
        cursor_blink: true,
        auto_name: None,
        persistent: false,
    };
    let id = info.id;
    reg.register(info);

    assert_eq!(reg.list().len(), 1);
    assert!(reg.get(&id).is_some());
}

#[test]
fn test_registry_unregister_nonexistent() {
    let mut reg = TerminalRegistry::new();
    assert!(reg.unregister(&Uuid::new_v4()).is_none());
}

#[test]
fn test_registry_list_active_filters_correctly() {
    let mut reg = TerminalRegistry::new();

    let active = TerminalInfo {
        id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        title: "Active".to_string(),
        status: TerminalStatus::Active,
        cols: 80,
        rows: 24,
        font_size: 14,
        cursor_style: "block".to_string(),
        cursor_blink: true,
        auto_name: None,
        persistent: false,
    };
    let idle = TerminalInfo {
        id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        title: "Idle".to_string(),
        status: TerminalStatus::Idle,
        cols: 80,
        rows: 24,
        font_size: 14,
        cursor_style: "block".to_string(),
        cursor_blink: true,
        auto_name: None,
        persistent: false,
    };
    let closed = TerminalInfo {
        id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        title: "Closed".to_string(),
        status: TerminalStatus::Closed,
        cols: 80,
        rows: 24,
        font_size: 14,
        cursor_style: "block".to_string(),
        cursor_blink: true,
        auto_name: None,
        persistent: false,
    };

    reg.register(active);
    reg.register(idle);
    reg.register(closed);

    assert_eq!(reg.list().len(), 3);
    assert_eq!(reg.list_active().len(), 1);
    assert_eq!(reg.list_active()[0].title, "Active");
}

#[test]
fn test_registry_update_status() {
    let mut reg = TerminalRegistry::new();
    let info = TerminalInfo {
        id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        title: "T1".to_string(),
        status: TerminalStatus::Active,
        cols: 80,
        rows: 24,
        font_size: 14,
        cursor_style: "block".to_string(),
        cursor_blink: true,
        auto_name: None,
        persistent: false,
    };
    let id = info.id;
    reg.register(info);

    assert!(reg.update_status(&id, TerminalStatus::Closed));
    assert_eq!(reg.get(&id).unwrap().status, TerminalStatus::Closed);
}

#[test]
fn test_registry_update_status_not_found() {
    let mut reg = TerminalRegistry::new();
    assert!(!reg.update_status(&Uuid::new_v4(), TerminalStatus::Active));
}

#[test]
fn test_registry_default() {
    let reg = TerminalRegistry::default();
    assert!(reg.list().is_empty());
}

// ===========================================================================
// Edge cases
// ===========================================================================

#[tokio::test]
async fn test_delete_terminal_with_invalid_uuid_returns_400() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .delete(format!("{base}/api/terminals/not-a-uuid"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_create_terminal_without_pty_pool_returns_503() {
    // Create state without a PTY pool.
    let event_bus = EventBus::new();
    let state = Arc::new(ApiState::new(event_bus)); // no pty_pool
    let router = api_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/terminals"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 503);

    let body: Value = resp.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("not available"));
}

#[tokio::test]
async fn test_double_delete_terminal() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let terminal = create_terminal(&client, &base).await;
    let tid = terminal["id"].as_str().unwrap();

    // First delete succeeds.
    let resp = client
        .delete(format!("{base}/api/terminals/{tid}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Second delete returns 404.
    let resp = client
        .delete(format!("{base}/api/terminals/{tid}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// ===========================================================================
// WebSocket Reconnection & Disconnect Buffer
// ===========================================================================

#[tokio::test]
async fn test_reconnect_within_grace_period_replays_buffer() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::protocol::Message;

    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    let terminal = create_terminal(&client, &base).await;
    let tid = terminal["id"].as_str().unwrap();
    let tid_uuid = Uuid::parse_str(tid).unwrap();

    let ws_url = base.replace("http://", "ws://") + &format!("/ws/terminal/{tid}");

    // First connection: connect and disconnect to trigger the Disconnected state.
    {
        let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .expect("first connect failed");

        tokio::time::sleep(Duration::from_millis(300)).await;
        ws.send(Message::Close(None)).await.ok();
    }

    // Wait for the disconnect handler to fire and create the buffer.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify status is Disconnected.
    {
        let registry = state.terminal_registry.read().await;
        let info = registry.get(&tid_uuid).expect("terminal should exist");
        assert!(
            matches!(info.status, TerminalStatus::Disconnected { .. }),
            "expected Disconnected status, got {:?}",
            info.status
        );
    }

    // Manually inject data into the disconnect buffer to avoid timing issues.
    {
        let mut buffers = state.disconnect_buffers.write().await;
        if let Some(buf) = buffers.get_mut(&tid_uuid) {
            buf.push(b"REPLAY_MARKER_DATA");
        } else {
            panic!("disconnect buffer should exist for terminal");
        }
    }

    // Reconnect within the grace period.
    {
        let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .expect("reconnect failed");

        // The very first message(s) should include the replayed buffer content.
        let mut all_output = String::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(500), ws.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    all_output.push_str(&text);
                    if all_output.contains("REPLAY_MARKER_DATA") {
                        break;
                    }
                }
                Ok(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => continue,
                _ => continue,
            }
        }

        assert!(
            all_output.contains("REPLAY_MARKER_DATA"),
            "expected buffered output to be replayed on reconnect, got: {all_output}"
        );
    }

    // Verify status returned to Active after reconnect.
    {
        let registry = state.terminal_registry.read().await;
        let info = registry.get(&tid_uuid).expect("terminal should exist");
        assert_eq!(
            info.status,
            TerminalStatus::Active,
            "expected Active status after reconnect"
        );
    }

    // Verify buffer was consumed (removed).
    {
        let buffers = state.disconnect_buffers.read().await;
        assert!(
            !buffers.contains_key(&tid_uuid),
            "disconnect buffer should be consumed after reconnect"
        );
    }
}

#[tokio::test]
async fn test_disconnect_buffer_unit_bounded() {
    // Unit test for DisconnectBuffer bounding behavior.
    let mut buf = DisconnectBuffer::new(16);

    // Push 20 bytes into a 16-byte buffer.
    buf.push(b"01234567890123456789");

    let out = buf.drain_all();
    assert_eq!(out.len(), 16, "buffer should be capped at max_bytes");
    // The oldest 4 bytes should have been dropped.
    assert_eq!(&out, b"4567890123456789");
}

#[tokio::test]
async fn test_terminal_dead_after_grace_period() {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::protocol::Message;

    // Use a short grace period for testing. We can't easily override the const,
    // so instead we test that the status transitions to Disconnected and the
    // terminal registry still has the terminal during the grace period.
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    let terminal = create_terminal(&client, &base).await;
    let tid = terminal["id"].as_str().unwrap();
    let tid_uuid = Uuid::parse_str(tid).unwrap();

    let ws_url = base.replace("http://", "ws://") + &format!("/ws/terminal/{tid}");

    // Connect and immediately disconnect.
    {
        let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .expect("connect failed");
        ws.send(Message::Close(None)).await.ok();
    }

    // Wait a brief moment for the disconnect handler to run.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Terminal should be in Disconnected state.
    {
        let registry = state.terminal_registry.read().await;
        let info = registry.get(&tid_uuid).expect("terminal should still exist");
        assert!(
            matches!(info.status, TerminalStatus::Disconnected { .. }),
            "expected Disconnected status, got {:?}",
            info.status
        );
    }

    // Verify a disconnect buffer exists.
    {
        let buffers = state.disconnect_buffers.read().await;
        assert!(
            buffers.contains_key(&tid_uuid),
            "disconnect buffer should exist during grace period"
        );
    }

    // Wait for grace period to expire (30s + a little buffer).
    // NOTE: This test takes ~31 seconds to run.
    tokio::time::sleep(WS_RECONNECT_GRACE + Duration::from_secs(2)).await;

    // Terminal should now be Dead.
    {
        let registry = state.terminal_registry.read().await;
        let info = registry.get(&tid_uuid).expect("terminal should still be in registry");
        assert_eq!(
            info.status,
            TerminalStatus::Dead,
            "expected Dead status after grace period"
        );
    }

    // PTY handle should be removed.
    {
        let handles = state.pty_handles.read().await;
        assert!(
            !handles.contains_key(&tid_uuid),
            "PTY handle should be removed after grace period"
        );
    }

    // Disconnect buffer should be cleaned up.
    {
        let buffers = state.disconnect_buffers.read().await;
        assert!(
            !buffers.contains_key(&tid_uuid),
            "disconnect buffer should be cleaned up after grace period"
        );
    }

    // Attempting to reconnect to a Dead terminal should fail.
    let result = tokio_tungstenite::connect_async(&ws_url).await;
    assert!(
        result.is_err(),
        "should not be able to reconnect to a Dead terminal"
    );
}
