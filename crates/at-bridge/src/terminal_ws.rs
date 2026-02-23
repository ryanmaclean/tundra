use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::http_api::ApiState;
use crate::terminal::{
    DisconnectBuffer, TerminalInfo, TerminalStatus, DISCONNECT_BUFFER_SIZE, WS_RECONNECT_GRACE,
};

/// Idle timeout for terminal WebSocket connections (5 minutes).
const WS_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// Heartbeat interval for terminal WebSocket connections (30 seconds).
/// Sends a Ping frame to detect half-open TCP connections.
const WS_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct TerminalResponse {
    pub id: String,
    pub title: String,
    pub status: String,
    pub cols: u16,
    pub rows: u16,
    pub font_size: u16,
    pub cursor_style: String,
    pub cursor_blink: bool,
    pub auto_name: Option<String>,
    pub persistent: bool,
}

impl From<&TerminalInfo> for TerminalResponse {
    fn from(info: &TerminalInfo) -> Self {
        Self {
            id: info.id.to_string(),
            title: info.title.clone(),
            status: match &info.status {
                TerminalStatus::Active => "active".to_string(),
                TerminalStatus::Idle => "idle".to_string(),
                TerminalStatus::Closed => "closed".to_string(),
                TerminalStatus::Disconnected { .. } => "disconnected".to_string(),
                TerminalStatus::Dead => "dead".to_string(),
            },
            cols: info.cols,
            rows: info.rows,
            font_size: info.font_size,
            cursor_style: info.cursor_style.clone(),
            cursor_blink: info.cursor_blink,
            auto_name: info.auto_name.clone(),
            persistent: info.persistent,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsIncoming {
    /// Raw input data to write to PTY stdin.
    Input { data: String },
    /// Resize the terminal.
    Resize { cols: u16, rows: u16 },
}

// ---------------------------------------------------------------------------
// REST Handlers
// ---------------------------------------------------------------------------

/// POST /api/terminals — spawn a new terminal session.
pub async fn create_terminal(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let pool = match &state.pty_pool {
        Some(pool) => pool.clone(),
        None => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "PTY pool not available"})),
            );
        }
    };

    // Spawn a shell process.
    let shell = if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/bash"
    };
    let handle = match pool.spawn(shell, &[], &[("TERM", "xterm-256color")]) {
        Ok(h) => h,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("spawn failed: {e}")})),
            );
        }
    };

    let terminal_id = handle.id;
    let info = TerminalInfo {
        id: terminal_id,
        agent_id: Uuid::nil(),
        title: format!("Terminal {}", &terminal_id.to_string()[..8]),
        status: TerminalStatus::Active,
        cols: 80,
        rows: 24,
        font_size: 14,
        cursor_style: "block".to_string(),
        cursor_blink: true,
        auto_name: None,
        persistent: false,
    };

    let resp = TerminalResponse::from(&info);

    // Register in the terminal registry.
    {
        let mut registry = state.terminal_registry.write().await;
        registry.register(info);
    }
    // Store the PTY handle.
    {
        let mut handles = state.pty_handles.write().await;
        handles.insert(terminal_id, handle);
    }

    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(resp)),
    )
}

/// GET /api/terminals — list active terminals.
pub async fn list_terminals(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let registry = state.terminal_registry.read().await;
    let terminals: Vec<TerminalResponse> = registry
        .list()
        .into_iter()
        .map(TerminalResponse::from)
        .collect();
    Json(serde_json::json!(terminals))
}

/// DELETE /api/terminals/{id} — kill a terminal session.
pub async fn delete_terminal(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let terminal_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid terminal ID"})),
            );
        }
    };

    // Remove from registry.
    {
        let mut registry = state.terminal_registry.write().await;
        if registry.unregister(&terminal_id).is_none() {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "terminal not found"})),
            );
        }
    }

    // Kill and remove the PTY handle.
    {
        let mut handles = state.pty_handles.write().await;
        if let Some(handle) = handles.remove(&terminal_id) {
            let _ = handle.kill();
        }
    }

    // Release from pool tracking.
    if let Some(pool) = &state.pty_pool {
        pool.release(terminal_id);
    }

    // Clean up any disconnect buffer.
    {
        let mut buffers = state.disconnect_buffers.write().await;
        buffers.remove(&terminal_id);
    }

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({"status": "deleted", "id": id})),
    )
}

// ---------------------------------------------------------------------------
// Rename Handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct AutoNameRequest {
    pub first_message: String,
}

/// POST /api/terminals/{id}/rename — rename a terminal session.
pub async fn rename_terminal(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(body): Json<RenameRequest>,
) -> impl IntoResponse {
    let terminal_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid terminal ID"})),
            );
        }
    };

    let mut registry = state.terminal_registry.write().await;
    if registry.rename(&terminal_id, body.name.clone()) {
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"status": "renamed", "id": id, "name": body.name})),
        )
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "terminal not found"})),
        )
    }
}

/// POST /api/terminals/{id}/auto-name — auto-generate a terminal name from first message.
pub async fn auto_name_terminal(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(body): Json<AutoNameRequest>,
) -> impl IntoResponse {
    let terminal_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid terminal ID"})),
            );
        }
    };

    // Extract up to 40 chars from the first message as the terminal name.
    let name: String = body.first_message.chars().take(40).collect();
    let name = name.trim().to_string();

    let mut registry = state.terminal_registry.write().await;
    if let Some(terminal) = registry.get_mut(&terminal_id) {
        terminal.auto_name = Some(name.clone());
        terminal.title = name.clone();
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"status": "auto_named", "id": id, "name": name})),
        )
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "terminal not found"})),
        )
    }
}

/// PATCH /api/terminals/{id}/settings — update terminal font/cursor settings.
pub async fn update_terminal_settings(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let terminal_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid terminal ID"})),
            );
        }
    };

    let mut registry = state.terminal_registry.write().await;
    if let Some(terminal) = registry.get_mut(&terminal_id) {
        if let Some(size) = req.get("font_size").and_then(|v| v.as_u64()) {
            terminal.font_size = size as u16;
        }
        if let Some(style) = req.get("cursor_style").and_then(|v| v.as_str()) {
            terminal.cursor_style = style.to_string();
        }
        if let Some(blink) = req.get("cursor_blink").and_then(|v| v.as_bool()) {
            terminal.cursor_blink = blink;
        }
        if let Some(persistent) = req.get("persistent").and_then(|v| v.as_bool()) {
            terminal.persistent = persistent;
        }
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"updated": terminal_id})),
        )
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "terminal not found"})),
        )
    }
}

/// GET /api/terminals/persistent — list persistent terminal sessions that should survive restart.
pub async fn list_persistent_terminals(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let registry = state.terminal_registry.read().await;
    let persistent: Vec<serde_json::Value> = registry
        .list_persistent()
        .into_iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "name": t.auto_name.as_deref().unwrap_or(&t.title),
                "font_size": t.font_size,
                "cursor_style": t.cursor_style,
            })
        })
        .collect();
    Json(persistent)
}

// ---------------------------------------------------------------------------
// WebSocket Handler
// ---------------------------------------------------------------------------

/// GET /ws/terminal/{id} — WebSocket for terminal I/O.
pub async fn terminal_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let terminal_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => {
            return (axum::http::StatusCode::BAD_REQUEST, "invalid terminal ID").into_response();
        }
    };

    // Check terminal exists and is not dead.
    {
        let registry = state.terminal_registry.read().await;
        match registry.get(&terminal_id) {
            None => {
                return (axum::http::StatusCode::NOT_FOUND, "terminal not found").into_response();
            }
            Some(info) if info.status == TerminalStatus::Dead => {
                return (
                    axum::http::StatusCode::GONE,
                    "terminal is dead (grace period expired)",
                )
                    .into_response();
            }
            _ => {}
        }
    }

    ws.on_upgrade(move |socket| handle_terminal_ws(socket, state, terminal_id))
        .into_response()
}

async fn handle_terminal_ws(socket: WebSocket, state: Arc<ApiState>, terminal_id: Uuid) {
    use futures_util::{SinkExt, StreamExt};

    let (ws_sender, mut ws_receiver) = socket.split();

    // Wrap ws_sender in Arc<Mutex> so both the reader task (PTY -> WS)
    // and the heartbeat task (Ping frames) can send through it.
    let ws_sender = Arc::new(tokio::sync::Mutex::new(ws_sender));

    // -----------------------------------------------------------------------
    // Replay buffered output if reconnecting to a Disconnected terminal.
    // -----------------------------------------------------------------------
    {
        let mut buffers = state.disconnect_buffers.write().await;
        if let Some(mut buf) = buffers.remove(&terminal_id) {
            let buffered = buf.drain_all();
            if !buffered.is_empty() {
                let text = String::from_utf8_lossy(&buffered).into_owned();
                tracing::info!(
                    %terminal_id,
                    bytes = buffered.len(),
                    "replaying disconnect buffer on reconnect"
                );
                let _ = ws_sender
                    .lock()
                    .await
                    .send(Message::Text(text.into()))
                    .await;
            }
        }
    }

    // Mark the terminal as Active (covers both fresh and reconnect cases).
    {
        let mut registry = state.terminal_registry.write().await;
        registry.update_status(&terminal_id, TerminalStatus::Active);
    }

    // Clone the reader channel from the PTY handle.
    let pty_reader = {
        let handles = state.pty_handles.read().await;
        match handles.get(&terminal_id) {
            Some(handle) => handle.reader.clone(),
            None => return,
        }
    };

    let pty_writer = {
        let handles = state.pty_handles.read().await;
        match handles.get(&terminal_id) {
            Some(handle) => handle.writer.clone(),
            None => return,
        }
    };

    // Task: PTY stdout -> WS (with 5-minute idle timeout)
    let ws_sender_reader = ws_sender.clone();
    let reader_task_handle = tokio::spawn(async move {
        loop {
            match tokio::time::timeout(WS_IDLE_TIMEOUT, pty_reader.recv_async()).await {
                Ok(Ok(data)) => {
                    let text = String::from_utf8_lossy(&data).into_owned();
                    if ws_sender_reader
                        .lock()
                        .await
                        .send(Message::Text(text.into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(Err(_)) => {
                    tracing::debug!("PTY reader closed");
                    break;
                }
                Err(_) => {
                    tracing::info!("terminal WebSocket idle timeout (5min), closing");
                    break;
                }
            }
        }
    });

    let reader_abort = reader_task_handle.abort_handle();

    // Task: WS -> PTY stdin (with 5-minute idle timeout)
    let writer_state = state.clone();
    let writer_terminal_id = terminal_id;
    let writer_task_handle = tokio::spawn(async move {
        loop {
            match tokio::time::timeout(WS_IDLE_TIMEOUT, ws_receiver.next()).await {
                Ok(Some(Ok(msg))) => {
                    match msg {
                        Message::Text(text) => {
                            // Try to parse as JSON command.
                            if let Ok(cmd) = serde_json::from_str::<WsIncoming>(&text) {
                                match cmd {
                                    WsIncoming::Input { data } => {
                                        let _ = pty_writer.send(data.into_bytes());
                                    }
                                    WsIncoming::Resize { cols, rows } => {
                                        tracing::debug!(
                                            %writer_terminal_id, cols, rows,
                                            "terminal resize requested"
                                        );

                                        // Resize the actual PTY via the master fd.
                                        {
                                            let handles = writer_state.pty_handles.read().await;
                                            if let Some(handle) = handles.get(&writer_terminal_id) {
                                                if let Err(e) = handle.resize(cols, rows) {
                                                    tracing::warn!(
                                                        %writer_terminal_id,
                                                        "PTY resize failed: {e}"
                                                    );
                                                }
                                            }
                                        }

                                        // Update the terminal registry dimensions.
                                        {
                                            let mut registry =
                                                writer_state.terminal_registry.write().await;
                                            if let Some(info) =
                                                registry.get_mut(&writer_terminal_id)
                                            {
                                                info.cols = cols;
                                                info.rows = rows;
                                            }
                                        }
                                    }
                                }
                            } else {
                                // Plain text: send directly as input.
                                let _ = pty_writer.send(text.as_bytes().to_vec());
                            }
                        }
                        Message::Binary(data) => {
                            let _ = pty_writer.send(data.to_vec());
                        }
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
                Ok(Some(Err(_))) | Ok(None) => break,
                Err(_) => {
                    tracing::info!("terminal WebSocket idle timeout (5min), closing");
                    break;
                }
            }
        }
    });

    let writer_abort = writer_task_handle.abort_handle();

    // Task: Heartbeat — send Ping every 30s to detect stale connections.
    // Pong responses are handled automatically by axum/tungstenite.
    let ws_sender_heartbeat = ws_sender.clone();
    let heartbeat_task_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(WS_HEARTBEAT_INTERVAL);
        loop {
            interval.tick().await;
            if ws_sender_heartbeat
                .lock()
                .await
                .send(Message::Ping(vec![].into()))
                .await
                .is_err()
            {
                tracing::debug!("heartbeat ping failed, connection lost");
                break;
            }
        }
    });

    let heartbeat_abort = heartbeat_task_handle.abort_handle();

    // Wait for any task to finish — then the connection is done.
    tokio::select! {
        _ = reader_task_handle => {},
        _ = writer_task_handle => {},
        _ = heartbeat_task_handle => {},
    }

    // Abort all spawned tasks so they stop consuming the PTY reader channel.
    // The reader_task must drop its Receiver clone before the background
    // buffer task can successfully read from the channel.
    reader_abort.abort();
    writer_abort.abort();
    heartbeat_abort.abort();
    // Yield to let the runtime process the aborts and drop task state.
    tokio::task::yield_now().await;

    // -----------------------------------------------------------------------
    // WS connection ended — enter Disconnected state and start buffering.
    // -----------------------------------------------------------------------
    tracing::info!(%terminal_id, "WebSocket disconnected, entering grace period");

    // Set status to Disconnected.
    {
        let mut registry = state.terminal_registry.write().await;
        registry.update_status(
            &terminal_id,
            TerminalStatus::Disconnected {
                since: chrono::Utc::now(),
            },
        );
    }

    // Create a disconnect buffer.
    {
        let mut buffers = state.disconnect_buffers.write().await;
        buffers.insert(terminal_id, DisconnectBuffer::new(DISCONNECT_BUFFER_SIZE));
    }

    // Clone the PTY reader again for the background buffer task.
    let pty_reader_bg = {
        let handles = state.pty_handles.read().await;
        match handles.get(&terminal_id) {
            Some(handle) => handle.reader.clone(),
            None => return, // PTY already gone
        }
    };

    // Spawn a background task that buffers PTY output during the grace period.
    let bg_state = state.clone();
    tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + WS_RECONNECT_GRACE;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }

            match tokio::time::timeout(remaining, pty_reader_bg.recv_async()).await {
                Ok(Ok(data)) => {
                    // Check if terminal was reconnected (buffer removed by new WS handler).
                    let mut buffers = bg_state.disconnect_buffers.write().await;
                    match buffers.get_mut(&terminal_id) {
                        Some(buf) => buf.push(&data),
                        None => {
                            // Buffer was consumed by a reconnecting client — stop.
                            tracing::debug!(
                                %terminal_id,
                                "disconnect buffer consumed, reconnect happened"
                            );
                            return;
                        }
                    }
                }
                Ok(Err(_)) => {
                    // PTY closed during grace period.
                    tracing::debug!(%terminal_id, "PTY closed during disconnect grace period");
                    break;
                }
                Err(_) => {
                    // Timeout — grace period expired.
                    break;
                }
            }
        }

        // Grace period expired (or PTY closed). Check if still disconnected.
        {
            let registry = bg_state.terminal_registry.read().await;
            if let Some(info) = registry.get(&terminal_id) {
                if !matches!(info.status, TerminalStatus::Disconnected { .. }) {
                    // Terminal was reconnected or otherwise handled — don't kill.
                    return;
                }
            } else {
                // Terminal was removed from registry.
                return;
            }
        }

        tracing::info!(
            %terminal_id,
            "reconnect grace period expired, killing terminal"
        );

        // Kill the PTY.
        {
            let mut handles = bg_state.pty_handles.write().await;
            if let Some(handle) = handles.remove(&terminal_id) {
                let _ = handle.kill();
            }
        }

        // Release from pool.
        if let Some(pool) = &bg_state.pty_pool {
            pool.release(terminal_id);
        }

        // Set status to Dead.
        {
            let mut registry = bg_state.terminal_registry.write().await;
            registry.update_status(&terminal_id, TerminalStatus::Dead);
        }

        // Clean up the buffer.
        {
            let mut buffers = bg_state.disconnect_buffers.write().await;
            buffers.remove(&terminal_id);
        }
    });
}

// Routes to add to http_api.rs api_router_with_auth:
// .route("/api/terminals/{id}/settings", patch(terminal_ws::update_terminal_settings))
// .route("/api/terminals/{id}/auto-name", post(terminal_ws::auto_name_terminal))
// .route("/api/terminals/persistent", get(terminal_ws::list_persistent_terminals))

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    /// Helper to create an ApiState with a PtyPool for testing.
    fn test_state() -> Arc<ApiState> {
        let event_bus = crate::event_bus::EventBus::new();
        let pool = Arc::new(at_session::pty_pool::PtyPool::new(4));
        Arc::new(ApiState::with_pty_pool(event_bus, pool))
    }

    fn test_app(state: Arc<ApiState>) -> axum::Router {
        crate::http_api::api_router(state)
    }

    #[tokio::test]
    async fn test_list_terminals_empty() {
        let state = test_state();
        let app = test_app(state);

        let req = Request::builder()
            .uri("/api/terminals")
            .method("GET")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let terminals: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(terminals.is_empty());
    }

    #[tokio::test]
    async fn test_create_terminal() {
        let state = test_state();
        let app = test_app(state);

        let req = Request::builder()
            .uri("/api/terminals")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let terminal: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(terminal.get("id").is_some());
        assert_eq!(terminal.get("status").unwrap().as_str().unwrap(), "active");
    }

    #[tokio::test]
    async fn test_delete_terminal_not_found() {
        let state = test_state();
        let app = test_app(state);

        let fake_id = Uuid::new_v4();
        let req = Request::builder()
            .uri(format!("/api/terminals/{fake_id}"))
            .method("DELETE")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_create_then_list_terminals() {
        let state = test_state();

        // Create a terminal.
        let app = test_app(state.clone());
        let req = Request::builder()
            .uri("/api/terminals")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        // List terminals — should have 1.
        let app = test_app(state);
        let req = Request::builder()
            .uri("/api/terminals")
            .method("GET")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let terminals: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(terminals.len(), 1);
    }

    #[tokio::test]
    async fn test_create_then_delete_terminal() {
        let state = test_state();

        // Create.
        let app = test_app(state.clone());
        let req = Request::builder()
            .uri("/api/terminals")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let terminal: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tid = terminal.get("id").unwrap().as_str().unwrap();

        // Delete.
        let app = test_app(state.clone());
        let req = Request::builder()
            .uri(format!("/api/terminals/{tid}"))
            .method("DELETE")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // List — should be empty.
        let app = test_app(state);
        let req = Request::builder()
            .uri("/api/terminals")
            .method("GET")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let terminals: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(terminals.is_empty());
    }
}
