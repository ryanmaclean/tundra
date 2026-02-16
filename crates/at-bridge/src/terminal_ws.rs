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
use uuid::Uuid;

use crate::http_api::ApiState;
use crate::terminal::{TerminalInfo, TerminalStatus};

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
}

impl From<&TerminalInfo> for TerminalResponse {
    fn from(info: &TerminalInfo) -> Self {
        Self {
            id: info.id.to_string(),
            title: info.title.clone(),
            status: format!("{:?}", info.status).to_lowercase(),
            cols: info.cols,
            rows: info.rows,
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
pub async fn create_terminal(
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
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
    let shell = if cfg!(target_os = "macos") { "/bin/zsh" } else { "/bin/bash" };
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

    (axum::http::StatusCode::CREATED, Json(serde_json::json!(resp)))
}

/// GET /api/terminals — list active terminals.
pub async fn list_terminals(
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
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

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({"status": "deleted", "id": id})),
    )
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

    // Check terminal exists.
    {
        let registry = state.terminal_registry.read().await;
        if registry.get(&terminal_id).is_none() {
            return (axum::http::StatusCode::NOT_FOUND, "terminal not found").into_response();
        }
    }

    ws.on_upgrade(move |socket| handle_terminal_ws(socket, state, terminal_id))
        .into_response()
}

async fn handle_terminal_ws(socket: WebSocket, state: Arc<ApiState>, terminal_id: Uuid) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

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

    use futures_util::{SinkExt, StreamExt};

    // Task: PTY stdout -> WS
    let reader_task = tokio::spawn(async move {
        loop {
            match pty_reader.recv_async().await {
                Ok(data) => {
                    let text = String::from_utf8_lossy(&data).into_owned();
                    if ws_sender.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Task: WS -> PTY stdin
    let writer_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            match msg {
                Message::Text(text) => {
                    // Try to parse as JSON command.
                    if let Ok(cmd) = serde_json::from_str::<WsIncoming>(&text) {
                        match cmd {
                            WsIncoming::Input { data } => {
                                let _ = pty_writer.send(data.into_bytes());
                            }
                            WsIncoming::Resize { cols, rows } => {
                                // Update the registry with new dimensions.
                                // Note: actual PTY resize would need the master fd,
                                // which portable-pty handles differently. For now we
                                // just track the dimensions.
                                tracing::debug!(
                                    %terminal_id, cols, rows,
                                    "terminal resize requested"
                                );
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
    });

    // Wait for either task to finish.
    tokio::select! {
        _ = reader_task => {},
        _ = writer_task => {},
    }
}

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

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let terminals: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(terminals.is_empty());
    }
}
