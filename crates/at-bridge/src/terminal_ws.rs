//! Terminal WebSocket handlers and REST API for managing PTY sessions.
//!
//! This module provides the HTTP REST API for terminal lifecycle management (create, list, delete)
//! and WebSocket handlers for interactive terminal I/O with resilient reconnection support.
//!
//! # WebSocket Protocol
//!
//! The terminal WebSocket endpoint at `GET /ws/terminal/{id}` provides a bidirectional channel
//! for terminal input and output. The protocol operates in two modes:
//!
//! ## Outgoing (Server → Client)
//!
//! - **Text Messages**: Terminal output (stdout/stderr) sent as UTF-8 text.
//! - **Ping Messages**: Heartbeat frames sent every 30 seconds to detect stale connections.
//!   Pong responses are handled automatically by the WebSocket library.
//!
//! ## Incoming (Client → Server)
//!
//! The client can send messages in two formats:
//!
//! ### 1. JSON Command Format (Structured)
//!
//! JSON-serialized [`WsIncoming`] commands for typed operations:
//!
//! ```json
//! {"type": "input", "data": "ls -la\n"}
//! ```
//!
//! ```json
//! {"type": "resize", "cols": 120, "rows": 30}
//! ```
//!
//! ### 2. Plain Text Format (Raw Input)
//!
//! Any text message that doesn't parse as JSON is treated as raw input and written
//! directly to the PTY stdin. This allows simple clients to send keystrokes without
//! JSON wrapping.
//!
//! ## Connection Lifecycle
//!
//! ### Active Connection
//!
//! When a WebSocket connection is established:
//! 1. Terminal status transitions to `Active`
//! 2. Any buffered output from a previous disconnection is replayed
//! 3. Three concurrent tasks are spawned:
//!    - **Reader**: Reads PTY output and sends to WebSocket (5-minute idle timeout)
//!    - **Writer**: Reads WebSocket messages and writes to PTY stdin (5-minute idle timeout)
//!    - **Heartbeat**: Sends Ping frames every 30 seconds to detect half-open connections
//!
//! ### Disconnection & Reconnection Grace Period
//!
//! When the WebSocket disconnects (network failure, tab close, etc.):
//! 1. Terminal status transitions to `Disconnected` with timestamp
//! 2. PTY process continues running in the background
//! 3. Output is buffered (last 4KB) for **10 seconds** ([`WS_RECONNECT_GRACE`])
//! 4. If client reconnects within grace period:
//!    - Buffered output is replayed to restore terminal state
//!    - Session resumes transparently
//! 5. If grace period expires without reconnection:
//!    - PTY process is killed
//!    - Terminal status transitions to `Dead`
//!    - Subsequent reconnect attempts receive 410 Gone
//!
//! This grace period ensures that brief network interruptions or page reloads don't
//! terminate long-running terminal sessions.
//!
//! ## Timeouts
//!
//! - **Idle Timeout**: 5 minutes (WS_IDLE_TIMEOUT) — WebSocket closes if no data flows in either direction
//! - **Heartbeat Interval**: 30 seconds (WS_HEARTBEAT_INTERVAL) — Ping frames detect half-open connections
//! - **Reconnect Grace**: 10 seconds ([`WS_RECONNECT_GRACE`]) — Buffer output after disconnect
//!
//! # REST API Endpoints
//!
//! ## Terminal Lifecycle
//!
//! - `POST /api/terminals` — [`create_terminal`] — Spawn a new terminal session
//! - `GET /api/terminals` — [`list_terminals`] — List all active terminals
//! - `DELETE /api/terminals/{id}` — [`delete_terminal`] — Kill a terminal session
//!
//! ## Terminal Management
//!
//! - `POST /api/terminals/{id}/rename` — [`rename_terminal`] — Set custom terminal name
//! - `POST /api/terminals/{id}/auto-name` — [`auto_name_terminal`] — Auto-generate name from first command
//! - `PATCH /api/terminals/{id}/settings` — [`update_terminal_settings`] — Update font size, cursor style, persistence
//! - `GET /api/terminals/persistent` — [`list_persistent_terminals`] — List persistent terminal sessions
//!
//! ## WebSocket
//!
//! - `GET /ws/terminal/{id}` — [`terminal_ws`] — Upgrade to WebSocket for terminal I/O
//!
//! # Examples
//!
//! ## Creating a Terminal and Connecting via WebSocket
//!
//! ```no_run
//! use reqwest;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // 1. Create terminal via REST API
//! let client = reqwest::Client::new();
//! let response = client.post("http://localhost:3000/api/terminals")
//!     .send()
//!     .await?;
//! let terminal: serde_json::Value = response.json().await?;
//! let terminal_id = terminal["id"].as_str().unwrap();
//!
//! // 2. Connect to WebSocket
//! // (Use a WebSocket client library to connect to ws://localhost:3000/ws/terminal/{terminal_id})
//!
//! // 3. Send input
//! // Send JSON: {"type": "input", "data": "echo hello\n"}
//!
//! // 4. Receive output
//! // Receive text messages with terminal output
//! # Ok(())
//! # }
//! ```

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use at_harness::security::{InputSanitizer, SecurityError};

use crate::http_api::ApiState;
use crate::origin_validation::{get_default_allowed_origins, validate_websocket_origin};
use crate::terminal::{
    DisconnectBuffer, TerminalInfo, TerminalStatus, DISCONNECT_BUFFER_SIZE, WS_RECONNECT_GRACE,
};

/// Idle timeout for terminal WebSocket connections (5 minutes).
///
/// If no data is sent or received on the WebSocket for this duration,
/// the connection is automatically closed. This prevents resource leaks
/// from abandoned connections.
const WS_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// Heartbeat interval for terminal WebSocket connections (30 seconds).
///
/// Ping frames are sent at this interval to detect half-open TCP connections
/// where the client has disconnected without sending a proper Close frame.
/// Pong responses are handled automatically by the WebSocket library.
const WS_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// REST API response representing a terminal session.
///
/// This structure is returned by terminal management endpoints like
/// [`create_terminal`] and [`list_terminals`]. It provides a snapshot
/// of the terminal's current state and configuration.
///
/// # Fields
///
/// - `id`: Unique terminal identifier (UUID)
/// - `title`: Display name for the terminal
/// - `status`: Current lifecycle state (active, idle, disconnected, closed, dead)
/// - `cols`, `rows`: Terminal dimensions in characters
/// - `font_size`: Font size in pixels
/// - `cursor_style`: Cursor appearance ("block", "underline", "bar")
/// - `cursor_blink`: Whether the cursor blinks
/// - `auto_name`: Auto-generated name from first command (if any)
/// - `persistent`: Whether this terminal should survive server restart
#[derive(Debug, Serialize)]
pub struct TerminalResponse {
    /// Unique terminal identifier (UUID).
    pub id: String,
    /// Display name for the terminal.
    pub title: String,
    /// Current lifecycle state (active, idle, disconnected, closed, dead).
    pub status: String,
    /// Terminal width in columns.
    pub cols: u16,
    /// Terminal height in rows.
    pub rows: u16,
    /// Font size in pixels.
    pub font_size: u16,
    pub font_family: String,
    pub line_height: f32,
    pub letter_spacing: f32,
    pub profile: String,
    /// Cursor appearance ("block", "underline", or "bar").
    pub cursor_style: String,
    /// Whether the cursor blinks.
    pub cursor_blink: bool,
    /// Auto-generated name from first command (if any).
    pub auto_name: Option<String>,
    /// Whether this terminal should survive server restart.
    pub persistent: bool,
}

impl From<&TerminalInfo> for TerminalResponse {
    /// Converts internal [`TerminalInfo`] to API response format.
    ///
    /// This mapping serializes the terminal status enum to a string
    /// and prepares all fields for JSON serialization.
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
            font_family: info.font_family.clone(),
            line_height: info.line_height,
            letter_spacing: info.letter_spacing,
            profile: info.profile.clone(),
            cursor_style: info.cursor_style.clone(),
            cursor_blink: info.cursor_blink,
            auto_name: info.auto_name.clone(),
            persistent: info.persistent,
        }
    }
}

/// WebSocket message types sent by clients to control terminal sessions.
///
/// This enum represents structured commands sent as JSON over the WebSocket.
/// Messages are tagged with a `type` field for discriminated union deserialization.
///
/// # Message Format
///
/// Messages use serde's `tag` attribute with `snake_case` field naming:
///
/// ```json
/// {"type": "input", "data": "ls -la\n"}
/// {"type": "resize", "cols": 120, "rows": 30}
/// ```
///
/// # Variants
///
/// - [`Input`](WsIncoming::Input): Send raw input data to PTY stdin (keystrokes, paste, etc.)
/// - [`Resize`](WsIncoming::Resize): Change terminal dimensions (triggers `SIGWINCH` to child process)
///
/// # Fallback Behavior
///
/// If a WebSocket text message doesn't parse as JSON, it's treated as raw input
/// and written directly to the PTY stdin. This allows simple clients to send
/// keystrokes without JSON wrapping.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsIncoming {
    /// Raw input data to write to PTY stdin.
    ///
    /// This variant handles user input like keystrokes, paste operations,
    /// or any data that should be written to the terminal's standard input.
    ///
    /// # Example
    ///
    /// ```json
    /// {"type": "input", "data": "echo hello\n"}
    /// ```
    Input {
        /// Raw input bytes as a UTF-8 string. Control characters like
        /// `\n` (Enter), `\t` (Tab), or `\x03` (Ctrl-C) are supported.
        data: String,
    },

    /// Resize the terminal dimensions.
    ///
    /// This triggers a PTY resize operation via `ioctl(TIOCSWINSZ)`, which
    /// sends `SIGWINCH` to the child process. Most shells and terminal applications
    /// handle this signal to adjust their display layout.
    ///
    /// # Example
    ///
    /// ```json
    /// {"type": "resize", "cols": 120, "rows": 30}
    /// ```
    Resize {
        /// New terminal width in character columns.
        cols: u16,
        /// New terminal height in character rows.
        rows: u16,
    },
}

// ---------------------------------------------------------------------------
// Helper Functions
// ---------------------------------------------------------------------------

/// Validates a text field using InputSanitizer to detect prompt injection and enforce length limits.
///
/// This helper function creates an InputSanitizer with default settings and validates
/// the provided text for potential security issues. It checks for:
/// - Maximum length violations (default: 10,000 characters)
/// - Prompt injection patterns (e.g., "ignore previous instructions", "system prompt:")
///
/// # Parameters
///
/// - `text`: The text field to validate
///
/// # Returns
///
/// - `Ok(String)`: The validated text (unchanged if validation passes)
/// - `Err(SecurityError)`: Validation failed due to security concerns
///
/// # Example
///
/// ```no_run
/// # use at_harness::security::SecurityError;
/// # fn validate_text_field(text: &str) -> Result<String, SecurityError> { Ok(text.to_string()) }
/// let user_input = "Hello, world!";
/// match validate_text_field(user_input) {
///     Ok(clean) => println!("Valid input: {}", clean),
///     Err(e) => eprintln!("Invalid input: {}", e),
/// }
/// ```
fn validate_text_field(text: &str) -> Result<String, SecurityError> {
    let sanitizer = InputSanitizer::new();
    sanitizer.sanitize(text)
}

// ---------------------------------------------------------------------------
// REST Handlers
// ---------------------------------------------------------------------------

/// `POST /api/terminals` — Spawn a new terminal session.
///
/// Creates a new PTY (pseudo-terminal) process running a shell and registers
/// it in the terminal registry. The shell is automatically selected based on
/// the operating system (zsh on macOS, bash elsewhere).
///
/// # Returns
///
/// - **201 Created**: Terminal created successfully
///   - Response body: [`TerminalResponse`] with terminal metadata
/// - **503 Service Unavailable**: PTY pool not available (server startup issue)
/// - **500 Internal Server Error**: Failed to spawn shell process
///
/// # Process Details
///
/// The spawned shell process:
/// - Runs with `TERM=xterm-256color` environment variable
/// - Starts with default dimensions: 80 columns × 24 rows
/// - Has a unique UUID identifier for WebSocket connection
/// - Begins in `Active` status
///
/// # Example
///
/// ```bash
/// curl -X POST http://localhost:3000/api/terminals
/// ```
///
/// Response:
/// ```json
/// {
///   "id": "550e8400-e29b-41d4-a716-446655440000",
///   "title": "Terminal 550e8400",
///   "status": "active",
///   "cols": 80,
///   "rows": 24,
///   "font_size": 14,
///   "cursor_style": "block",
///   "cursor_blink": true,
///   "auto_name": null,
///   "persistent": false
/// }
/// ```
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
        font_size: 12,
        font_family: "\"Iosevka Term\",\"JetBrains Mono\",\"SF Mono\",\"Menlo\",monospace"
            .to_string(),
        line_height: 1.02,
        letter_spacing: 0.15,
        profile: "bundled-card".to_string(),
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

/// `GET /api/terminals` — List all active terminal sessions.
///
/// Returns an array of all terminal sessions in the registry, regardless of status
/// (active, disconnected, idle, etc.). Dead terminals are retained in the registry
/// to allow clients to detect expired sessions.
///
/// # Returns
///
/// - **200 OK**: Array of [`TerminalResponse`] objects
///
/// # Example
///
/// ```bash
/// curl http://localhost:3000/api/terminals
/// ```
///
/// Response:
/// ```json
/// [
///   {
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "title": "Terminal 550e8400",
///     "status": "active",
///     "cols": 80,
///     "rows": 24,
///     ...
///   }
/// ]
/// ```
pub async fn list_terminals(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let registry = state.terminal_registry.read().await;
    let terminals: Vec<TerminalResponse> = registry
        .list()
        .into_iter()
        .map(TerminalResponse::from)
        .collect();
    Json(serde_json::json!(terminals))
}

/// `DELETE /api/terminals/{id}` — Kill a terminal session and clean up resources.
///
/// Forcefully terminates the terminal's PTY process, removes it from the registry,
/// releases pool resources, and cleans up any disconnect buffers. This is the
/// proper way to close a terminal session.
///
/// # Path Parameters
///
/// - `id`: Terminal UUID (e.g., `550e8400-e29b-41d4-a716-446655440000`)
///
/// # Returns
///
/// - **200 OK**: Terminal deleted successfully
/// - **400 Bad Request**: Invalid UUID format
/// - **404 Not Found**: Terminal not found in registry
///
/// # Cleanup Operations
///
/// This handler performs the following cleanup:
/// 1. Unregister terminal from registry
/// 2. Kill PTY child process (sends `SIGKILL`)
/// 3. Remove PTY handle from tracking map
/// 4. Release terminal ID from pool
/// 5. Remove any disconnect buffer
///
/// # Example
///
/// ```bash
/// curl -X DELETE http://localhost:3000/api/terminals/550e8400-e29b-41d4-a716-446655440000
/// ```
///
/// Response:
/// ```json
/// {"status": "deleted", "id": "550e8400-e29b-41d4-a716-446655440000"}
/// ```
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

/// Request body for renaming a terminal.
///
/// Used by the [`rename_terminal`] endpoint to set a custom display name.
#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    /// New display name for the terminal.
    pub name: String,
}

/// Request body for auto-generating a terminal name.
///
/// Used by the [`auto_name_terminal`] endpoint to generate a name
/// from the first command sent to the terminal.
#[derive(Debug, Deserialize)]
pub struct AutoNameRequest {
    /// The first command or message sent to the terminal.
    /// Up to 40 characters will be extracted and used as the name.
    pub first_message: String,
}

/// `POST /api/terminals/{id}/rename` — Set a custom terminal name.
///
/// Updates the terminal's display name in the registry. This name is shown
/// in the UI and persists until changed again or the terminal is deleted.
///
/// # Path Parameters
///
/// - `id`: Terminal UUID
///
/// # Request Body
///
/// [`RenameRequest`] JSON object:
/// ```json
/// {"name": "Backend Server"}
/// ```
///
/// # Returns
///
/// - **200 OK**: Terminal renamed successfully
/// - **400 Bad Request**: Invalid UUID format
/// - **404 Not Found**: Terminal not found in registry
///
/// # Example
///
/// ```bash
/// curl -X POST http://localhost:3000/api/terminals/550e8400.../rename \
///   -H "Content-Type: application/json" \
///   -d '{"name": "Backend Server"}'
/// ```
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

    // Validate the name field
    if let Err(e) = validate_text_field(&body.name) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        );
    }

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

/// `POST /api/terminals/{id}/auto-name` — Auto-generate terminal name from first command.
///
/// Extracts up to 40 characters from the first command or message sent to the terminal
/// and uses it as the terminal's display name. This provides context about what the
/// terminal is being used for (e.g., "npm run dev", "docker logs -f").
///
/// # Path Parameters
///
/// - `id`: Terminal UUID
///
/// # Request Body
///
/// [`AutoNameRequest`] JSON object:
/// ```json
/// {"first_message": "npm run dev -- --port 3000"}
/// ```
///
/// # Returns
///
/// - **200 OK**: Terminal auto-named successfully
/// - **400 Bad Request**: Invalid UUID format
/// - **404 Not Found**: Terminal not found in registry
///
/// # Behavior
///
/// - Extracts up to 40 characters from `first_message`
/// - Trims leading/trailing whitespace
/// - Sets both `title` and `auto_name` fields in the terminal registry
///
/// # Example
///
/// ```bash
/// curl -X POST http://localhost:3000/api/terminals/550e8400.../auto-name \
///   -H "Content-Type: application/json" \
///   -d '{"first_message": "npm run dev"}'
/// ```
///
/// Response:
/// ```json
/// {"status": "auto_named", "id": "550e8400...", "name": "npm run dev"}
/// ```
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

    // Validate the first_message field
    if let Err(e) = validate_text_field(&body.first_message) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        );
    }

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

/// `PATCH /api/terminals/{id}/settings` — Update terminal display and persistence settings.
///
/// Allows partial updates to terminal configuration like font size, cursor style,
/// cursor blinking, and persistence flag. Only provided fields are updated.
///
/// # Path Parameters
///
/// - `id`: Terminal UUID
///
/// # Request Body
///
/// JSON object with optional fields:
/// ```json
/// {
///   "font_size": 16,
///   "cursor_style": "bar",
///   "cursor_blink": false,
///   "persistent": true
/// }
/// ```
///
/// # Supported Fields
///
/// - `font_size` (u16): Font size in pixels (e.g., 12, 14, 16)
/// - `cursor_style` (string): Cursor appearance ("block", "underline", "bar")
/// - `cursor_blink` (bool): Whether the cursor should blink
/// - `persistent` (bool): Whether terminal should survive server restart
///
/// # Returns
///
/// - **200 OK**: Settings updated successfully
/// - **400 Bad Request**: Invalid UUID format
/// - **404 Not Found**: Terminal not found in registry
///
/// # Example
///
/// ```bash
/// curl -X PATCH http://localhost:3000/api/terminals/550e8400.../settings \
///   -H "Content-Type: application/json" \
///   -d '{"font_size": 16, "persistent": true}'
/// ```
///
/// Response:
/// ```json
/// {"updated": "550e8400-e29b-41d4-a716-446655440000"}
/// ```
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
        if let Some(font) = req.get("font_family").and_then(|v| v.as_str()) {
            terminal.font_family = font.to_string();
        }
        if let Some(line_height) = req.get("line_height").and_then(|v| v.as_f64()) {
            terminal.line_height = line_height as f32;
        }
        if let Some(letter_spacing) = req.get("letter_spacing").and_then(|v| v.as_f64()) {
            terminal.letter_spacing = letter_spacing as f32;
        }
        if let Some(profile) = req.get("profile").and_then(|v| v.as_str()) {
            terminal.profile = profile.to_string();
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

/// `GET /api/terminals/persistent` — List persistent terminal sessions.
///
/// Returns terminals marked with `persistent: true`, which indicates they should
/// be preserved across server restarts. This endpoint is typically used during
/// server initialization to restore terminal sessions.
///
/// # Returns
///
/// - **200 OK**: Array of persistent terminal metadata
///
/// # Response Format
///
/// Returns a simplified view of persistent terminals:
/// ```json
/// [
///   {
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "name": "Backend Server",
///     "font_size": 14,
///     "cursor_style": "block"
///   }
/// ]
/// ```
///
/// # Example
///
/// ```bash
/// curl http://localhost:3000/api/terminals/persistent
/// ```
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
                "font_family": t.font_family,
                "line_height": t.line_height,
                "letter_spacing": t.letter_spacing,
                "profile": t.profile,
                "cursor_style": t.cursor_style,
            })
        })
        .collect();
    Json(persistent)
}

// ---------------------------------------------------------------------------
// WebSocket Handler
// ---------------------------------------------------------------------------

/// `GET /ws/terminal/{id}` — Upgrade HTTP connection to WebSocket for terminal I/O.
///
/// This is the entry point for establishing an interactive terminal session via WebSocket.
/// It validates the terminal ID, checks the terminal status, and upgrades the connection
/// to WebSocket protocol.
///
/// # Path Parameters
///
/// - `id`: Terminal UUID (must match an existing terminal)
///
/// # Returns
///
/// - **101 Switching Protocols**: WebSocket upgrade successful
/// - **400 Bad Request**: Invalid UUID format
/// - **404 Not Found**: Terminal not found in registry
/// - **410 Gone**: Terminal is dead (grace period expired, no reconnection allowed)
///
/// # Connection States
///
/// ## Terminal Active or Disconnected
///
/// WebSocket upgrade proceeds normally. If the terminal is `Disconnected`,
/// any buffered output from the previous connection will be replayed.
///
/// ## Terminal Dead
///
/// Returns 410 Gone, indicating the terminal's grace period expired and
/// the PTY process was killed. The client must create a new terminal.
///
/// # Protocol
///
/// See module-level documentation for details on the WebSocket protocol,
/// message formats, and reconnection behavior.
///
/// # Example
///
/// ```javascript
/// // JavaScript WebSocket client
/// const ws = new WebSocket('ws://localhost:3000/ws/terminal/550e8400-e29b-41d4-a716-446655440000');
///
/// ws.onmessage = (event) => {
///   console.log('Terminal output:', event.data);
/// };
///
/// ws.send(JSON.stringify({type: 'input', data: 'ls -la\n'}));
/// ```
pub async fn terminal_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Validate Origin header to prevent cross-site WebSocket hijacking.
    let allowed_origins = get_default_allowed_origins();
    if let Err(status) = validate_websocket_origin(&headers, &allowed_origins) {
        return (status, "origin not allowed").into_response();
    }

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

/// Internal handler for managing WebSocket connection lifecycle.
///
/// This function implements the core WebSocket protocol logic, including:
/// - Reconnection buffer replay
/// - Bidirectional I/O between WebSocket and PTY
/// - Heartbeat/keepalive mechanism
/// - Idle timeout enforcement
/// - Graceful disconnection with buffering
///
/// # Architecture
///
/// The handler spawns three concurrent tasks:
///
/// ## 1. Reader Task (PTY → WebSocket)
///
/// Reads output from the PTY's stdout/stderr channel and forwards it to the WebSocket
/// as text messages. Applies a 5-minute idle timeout — if the PTY produces no output
/// for 5 minutes, the WebSocket connection is closed to free resources.
///
/// ## 2. Writer Task (WebSocket → PTY)
///
/// Reads messages from the WebSocket and processes them:
/// - JSON-formatted [`WsIncoming`] commands for typed operations (input, resize)
/// - Plain text messages treated as raw PTY input
///
/// Also applies a 5-minute idle timeout on the receive side.
///
/// ## 3. Heartbeat Task
///
/// Sends WebSocket Ping frames every 30 seconds to detect half-open connections.
/// If the client has disconnected without sending a Close frame (e.g., network failure),
/// the Ping will fail and trigger connection cleanup.
///
/// # Reconnection Logic
///
/// When this handler is invoked:
/// 1. Check for existing disconnect buffer (indicates reconnection)
/// 2. If buffer exists, replay all buffered output to restore terminal state
/// 3. Transition terminal status from `Disconnected` → `Active`
///
/// This allows clients to seamlessly resume sessions after brief network interruptions.
///
/// # Disconnection Handling
///
/// When any task exits (idle timeout, client disconnect, heartbeat failure):
/// 1. Abort all spawned tasks to release PTY reader resources
/// 2. Transition terminal status to `Disconnected` with timestamp
/// 3. Create disconnect buffer (4KB ring buffer for PTY output)
/// 4. Spawn background task to buffer output for 10 seconds
/// 5. If no reconnection occurs, kill PTY and mark terminal `Dead`
///
/// # Grace Period Details
///
/// The 10-second grace period ([`WS_RECONNECT_GRACE`]) allows clients to:
/// - Recover from transient network failures
/// - Reload the page without losing session
/// - Switch tabs without session termination
///
/// During this period, PTY output is buffered (last 4KB) and replayed on reconnection.
/// If the grace period expires, the PTY process is killed and subsequent reconnection
/// attempts receive 410 Gone.
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

    // -----------------------------------------------------------------------
    // Task 1: PTY stdout -> WebSocket (with 5-minute idle timeout)
    // -----------------------------------------------------------------------
    // Forwards PTY output to the WebSocket client. If no output is produced
    // for WS_IDLE_TIMEOUT (5 minutes), the connection is closed to prevent
    // resource leaks from idle terminals.
    let ws_sender_reader = ws_sender.clone();
    let reader_task_handle = tokio::spawn(async move {
        loop {
            match tokio::time::timeout(WS_IDLE_TIMEOUT, pty_reader.recv_async()).await {
                Ok(Ok(data)) => {
                    // Convert raw bytes to UTF-8 (with lossy conversion for invalid sequences).
                    let text = String::from_utf8_lossy(&data).into_owned();
                    if ws_sender_reader
                        .lock()
                        .await
                        .send(Message::Text(text.into()))
                        .await
                        .is_err()
                    {
                        // WebSocket send failed — client disconnected.
                        break;
                    }
                }
                Ok(Err(_)) => {
                    // PTY reader channel closed — child process exited.
                    tracing::debug!("PTY reader closed");
                    break;
                }
                Err(_) => {
                    // Idle timeout expired — no output for 5 minutes.
                    tracing::info!("terminal WebSocket idle timeout (5min), closing");
                    break;
                }
            }
        }
    });

    let reader_abort = reader_task_handle.abort_handle();

    // -----------------------------------------------------------------------
    // Task 2: WebSocket -> PTY stdin (with 5-minute idle timeout)
    // -----------------------------------------------------------------------
    // Receives messages from the WebSocket client and forwards them to the PTY.
    // Supports both structured JSON commands (WsIncoming) and plain text input.
    let writer_state = state.clone();
    let writer_terminal_id = terminal_id;
    let writer_task_handle = tokio::spawn(async move {
        loop {
            match tokio::time::timeout(WS_IDLE_TIMEOUT, ws_receiver.next()).await {
                Ok(Some(Ok(msg))) => {
                    match msg {
                        Message::Text(text) => {
                            // Try to parse as JSON command first.
                            if let Ok(cmd) = serde_json::from_str::<WsIncoming>(&text) {
                                match cmd {
                                    WsIncoming::Input { data } => {
                                        // Write raw input to PTY stdin.
                                        let _ = pty_writer.send(data.into_bytes());
                                    }
                                    WsIncoming::Resize { cols, rows } => {
                                        tracing::debug!(
                                            %writer_terminal_id, cols, rows,
                                            "terminal resize requested"
                                        );

                                        // Resize the actual PTY via ioctl(TIOCSWINSZ).
                                        // This sends SIGWINCH to the child process.
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

                                        // Update the terminal registry dimensions for display.
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
                                // Not JSON — treat as plain text input.
                                // This allows simple clients to send keystrokes without JSON wrapping.
                                let _ = pty_writer.send(text.as_bytes().to_vec());
                            }
                        }
                        Message::Binary(data) => {
                            // Binary data forwarded directly to PTY stdin.
                            let _ = pty_writer.send(data.to_vec());
                        }
                        Message::Close(_) => break,
                        _ => {
                            // Ignore other message types (Ping, Pong — handled automatically).
                        }
                    }
                }
                Ok(Some(Err(_))) | Ok(None) => {
                    // WebSocket stream error or closed.
                    break;
                }
                Err(_) => {
                    // Idle timeout expired — no messages received for 5 minutes.
                    tracing::info!("terminal WebSocket idle timeout (5min), closing");
                    break;
                }
            }
        }
    });

    let writer_abort = writer_task_handle.abort_handle();

    // -----------------------------------------------------------------------
    // Task 3: Heartbeat — send Ping frames every 30 seconds
    // -----------------------------------------------------------------------
    // Detects half-open TCP connections where the client has disconnected
    // without sending a Close frame (e.g., network failure, process kill).
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
                // Ping send failed — connection is dead.
                tracing::debug!("heartbeat ping failed, connection lost");
                break;
            }
        }
    });

    let heartbeat_abort = heartbeat_task_handle.abort_handle();

    // -----------------------------------------------------------------------
    // Wait for any task to complete — then the connection is done.
    // -----------------------------------------------------------------------
    tokio::select! {
        _ = reader_task_handle => {},
        _ = writer_task_handle => {},
        _ = heartbeat_task_handle => {},
    }

    // Abort all spawned tasks to release resources immediately.
    // CRITICAL: The reader_task must drop its Receiver clone before the
    // background buffer task can successfully read from the channel.
    // Without this, the buffer task would hang waiting for the reader_task
    // to release the channel.
    reader_abort.abort();
    writer_abort.abort();
    heartbeat_abort.abort();

    // Yield to the runtime to ensure aborts are processed and task state is dropped.
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

    // -----------------------------------------------------------------------
    // Spawn background task to buffer PTY output during grace period.
    // -----------------------------------------------------------------------
    // This task continues reading PTY output for WS_RECONNECT_GRACE (30 seconds)
    // and buffers it (last 64KB) in case the client reconnects. If the grace
    // period expires without reconnection, the PTY is killed and the terminal
    // transitions to Dead status.
    //
    // If the PTY process exits before the grace period ends, the task
    // immediately cleans up the registry and disconnect buffer rather than
    // waiting for the periodic TTL-based cleanup cycle.
    let bg_state = state.clone();
    tokio::spawn(spawn_reconnect_grace_task(
        bg_state,
        terminal_id,
        pty_reader_bg,
    ));
}

/// Background task that buffers PTY output during a WebSocket reconnect grace period.
///
/// This task runs after a WebSocket disconnects. It reads PTY output and stores it
/// in the disconnect buffer so that a reconnecting client can replay missed output.
///
/// # Termination paths
///
/// 1. **Grace period expired** (timeout arm): the 30-second window closed without
///    reconnection. The PTY is killed and the terminal transitions to `Dead`.
///
/// 2. **PTY process exited early** (`Ok(Err(_))` arm): the PTY reader channel
///    closed because the child process exited before the grace period ended.
///    The terminal is immediately transitioned to `Dead` and all registry /
///    disconnect-buffer entries are removed — without waiting for the periodic
///    TTL-based cleanup cycle. Failing to do so would leave a zombie
///    `TerminalStatus::Disconnected` entry and a dangling `DisconnectBuffer`
///    until the next cleanup tick.
///
/// 3. **Client reconnected** (buffer consumed): a reconnecting client drained
///    the disconnect buffer. The task returns without further action.
async fn spawn_reconnect_grace_task(
    state: Arc<ApiState>,
    terminal_id: Uuid,
    pty_reader: flume::Receiver<Vec<u8>>,
) {
    let deadline = tokio::time::Instant::now() + WS_RECONNECT_GRACE;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            // Grace period expired.
            break;
        }

        match tokio::time::timeout(remaining, pty_reader.recv_async()).await {
            Ok(Ok(data)) => {
                // PTY produced output — add to disconnect buffer.
                let mut buffers = state.disconnect_buffers.write().await;
                match buffers.get_mut(&terminal_id) {
                    Some(buf) => {
                        // Buffer exists — push data (ring buffer overwrites oldest data).
                        buf.push(&data);
                    }
                    None => {
                        // Buffer was consumed by a reconnecting client — stop buffering.
                        tracing::debug!(
                            %terminal_id,
                            "disconnect buffer consumed, reconnect happened"
                        );
                        return;
                    }
                }
            }
            Ok(Err(_)) => {
                // PTY reader channel closed — child process exited during grace period.
                //
                // Do NOT fall through to the shared "grace period expired" path below.
                // That path guards on `TerminalStatus::Disconnected` and could be
                // skipped by a concurrent status change, leaving a zombie entry.
                // Instead clean up immediately here.
                tracing::debug!(%terminal_id, "PTY closed during disconnect grace period, cleaning up");
                cleanup_disconnected_terminal(&state, terminal_id).await;
                return;
            }
            Err(_) => {
                // Timeout — grace period expired without PTY output or reconnect.
                break;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Grace period expired. Check if still disconnected before killing.
    // -----------------------------------------------------------------------
    {
        let registry = state.terminal_registry.read().await;
        if let Some(info) = registry.get(&terminal_id) {
            if !matches!(info.status, TerminalStatus::Disconnected { .. }) {
                // Terminal was reconnected or manually deleted — don't kill.
                return;
            }
        } else {
            // Terminal was removed from registry (manually deleted).
            return;
        }
    }

    tracing::info!(
        %terminal_id,
        "reconnect grace period expired, killing terminal"
    );

    cleanup_disconnected_terminal(&state, terminal_id).await;
}

/// Kill the PTY and remove all registry / buffer state for a disconnected terminal.
///
/// Called from two code paths:
/// - After the grace period expires with no reconnection.
/// - When the PTY reader channel closes early (child process exited).
///
/// Idempotent: missing entries in `pty_handles` or `disconnect_buffers` are
/// silently ignored.
async fn cleanup_disconnected_terminal(state: &Arc<ApiState>, terminal_id: Uuid) {
    // Kill the PTY child process (sends SIGKILL).
    {
        let mut handles = state.pty_handles.write().await;
        if let Some(handle) = handles.remove(&terminal_id) {
            let _ = handle.kill();
        }
    }

    // Release terminal ID from pool tracking.
    if let Some(pool) = &state.pty_pool {
        pool.release(terminal_id);
    }

    // Set status to Dead — subsequent reconnect attempts will receive 410 Gone.
    {
        let mut registry = state.terminal_registry.write().await;
        registry.update_status(&terminal_id, TerminalStatus::Dead);
    }

    // Clean up the disconnect buffer.
    {
        let mut buffers = state.disconnect_buffers.write().await;
        buffers.remove(&terminal_id);
    }
}

// Routes to add to http_api.rs api_router_with_auth:
// .route("/api/terminals/{id}/settings", patch(terminal_ws::update_terminal_settings))
// .route("/api/terminals/{id}/auto-name", post(terminal_ws::auto_name_terminal))
// .route("/api/terminals/persistent", get(terminal_ws::list_persistent_terminals))

// ---------------------------------------------------------------------------
// Concurrency / stress tests
//
// Pin the hard contracts of the terminal WebSocket layer under racy conditions:
// concurrent registry reads and writes, racing disconnect-buffer drain vs push,
// concurrent multi-terminal status updates, and concurrent registry insertions.
//
// Implementation notes:
//   * `TerminalRegistry` lives behind `Arc<RwLock<...>>` in `ApiState`; all
//     concurrent access goes through standard tokio RwLock semantics.
//   * `DisconnectBuffer` itself is not `Send` on its own and is always accessed
//     through the `Arc<RwLock<HashMap<...>>>` wrapper in `ApiState`.
//   * These tests use `tokio::test` with `flavor = "multi_thread"` and
//     `JoinSet`/`spawn` — consistent with the `oauth_token_race` integration
//     test layout but placed here as an in-module `stress_tests` mod to match
//     the `event_bus` pattern.
//   * Each test stays well under 2 s wall-clock time. No real sleep in the
//     hot assertion path; `AtomicUsize` counters + `tokio::task::yield_now`
//     are used for synchronisation.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod stress_tests {
    use super::*;
    use crate::event_bus::EventBus;
    use crate::terminal::{DisconnectBuffer, TerminalInfo, TerminalStatus, DISCONNECT_BUFFER_SIZE};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::task::JoinSet;
    use uuid::Uuid;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_state() -> Arc<ApiState> {
        Arc::new(ApiState::new(EventBus::new()))
    }

    fn make_terminal_info(status: TerminalStatus) -> TerminalInfo {
        TerminalInfo {
            id: Uuid::new_v4(),
            agent_id: Uuid::nil(),
            title: "stress-test terminal".to_string(),
            status,
            cols: 80,
            rows: 24,
            font_size: 12,
            font_family: "monospace".to_string(),
            line_height: 1.0,
            letter_spacing: 0.0,
            profile: "bundled-card".to_string(),
            cursor_style: "block".to_string(),
            cursor_blink: true,
            auto_name: None,
            persistent: false,
        }
    }

    // -----------------------------------------------------------------------
    // S1: concurrent registry insertions never corrupt state
    //
    // N tasks simultaneously register distinct terminals. After all complete,
    // every terminal must be individually retrievable and the total count must
    // equal N. Pins: no entries are lost or overwritten by concurrent
    // `RwLock::write` acquisitions.
    // -----------------------------------------------------------------------
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_registry_insertions_preserve_all_entries() {
        const N: usize = 32;
        let state = make_state();

        let mut ids = Vec::with_capacity(N);
        let mut set = JoinSet::new();

        for _ in 0..N {
            let info = make_terminal_info(TerminalStatus::Active);
            ids.push(info.id);
            let state_c = Arc::clone(&state);
            set.spawn(async move {
                let mut registry = state_c.terminal_registry.write().await;
                registry.register(info);
            });
        }

        while set.join_next().await.is_some() {}

        let registry = state.terminal_registry.read().await;
        assert_eq!(
            registry.list().len(),
            N,
            "all {N} concurrent insertions must survive"
        );
        for id in &ids {
            assert!(
                registry.get(id).is_some(),
                "terminal {id} must be retrievable after concurrent insert"
            );
        }
    }

    // -----------------------------------------------------------------------
    // S2: concurrent readers never block each other or observe partial state
    //
    // Pre-populate M terminals, then spawn N concurrent reader tasks that each
    // call `registry.list()`. All readers must see exactly M entries.
    // Pins: `RwLock::read` allows true fan-out; no reader observes a torn list.
    // -----------------------------------------------------------------------
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_readers_all_observe_complete_registry() {
        const M: usize = 16; // terminals pre-inserted
        const N: usize = 32; // concurrent reader tasks

        let state = make_state();
        {
            let mut registry = state.terminal_registry.write().await;
            for _ in 0..M {
                registry.register(make_terminal_info(TerminalStatus::Active));
            }
        }

        let mut set = JoinSet::new();
        let observed = Arc::new(AtomicUsize::new(0));

        for _ in 0..N {
            let state_c = Arc::clone(&state);
            let observed_c = Arc::clone(&observed);
            set.spawn(async move {
                let registry = state_c.terminal_registry.read().await;
                let count = registry.list().len();
                // Each reader must see the full pre-populated set.
                observed_c.fetch_add(count, Ordering::Relaxed);
            });
        }

        while set.join_next().await.is_some() {}

        assert_eq!(
            observed.load(Ordering::Relaxed),
            M * N,
            "each of {N} readers must see all {M} terminals (total {total})",
            total = M * N
        );
    }

    // -----------------------------------------------------------------------
    // S3: racing reader and writer tasks do not deadlock or panic
    //
    // One writer task continuously inserts terminals while N reader tasks
    // continuously read the registry list. The test is bounded by a task
    // count; post-condition is that the writer completed and the registry
    // is non-empty (count ≥ 1). Pins: no deadlock between concurrent
    // `read()` and `write()` acquisitions on the same RwLock.
    // -----------------------------------------------------------------------
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn registry_read_write_race_no_deadlock() {
        const WRITES: usize = 20;
        const READERS: usize = 8;

        let state = make_state();
        let inserted = Arc::new(AtomicUsize::new(0));

        // Writer task.
        let state_w = Arc::clone(&state);
        let inserted_w = Arc::clone(&inserted);
        let writer = tokio::spawn(async move {
            for _ in 0..WRITES {
                let info = make_terminal_info(TerminalStatus::Active);
                let mut registry = state_w.terminal_registry.write().await;
                registry.register(info);
                inserted_w.fetch_add(1, Ordering::Relaxed);
                tokio::task::yield_now().await;
            }
        });

        // Reader tasks run concurrently with the writer.
        let mut set = JoinSet::new();
        for _ in 0..READERS {
            let state_c = Arc::clone(&state);
            set.spawn(async move {
                for _ in 0..20 {
                    let registry = state_c.terminal_registry.read().await;
                    let _ = registry.list();
                    drop(registry);
                    tokio::task::yield_now().await;
                }
            });
        }

        writer.await.expect("writer task must not panic");
        while set.join_next().await.is_some() {}

        let registry = state.terminal_registry.read().await;
        assert_eq!(
            registry.list().len(),
            inserted.load(Ordering::Relaxed),
            "all inserted terminals must be present after racing reads"
        );
    }

    // -----------------------------------------------------------------------
    // S4: concurrent status updates on distinct terminals do not interfere
    //
    // N tasks each own one terminal and independently update its status.
    // After all complete, each terminal must reflect the status its owner
    // wrote, not a neighbour's. Pins: `update_status` with write lock is
    // per-key atomic — no cross-terminal write bleeding.
    // -----------------------------------------------------------------------
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_status_updates_on_distinct_terminals_are_isolated() {
        const N: usize = 16;
        let state = make_state();

        // Pre-register all terminals.
        let mut ids = Vec::with_capacity(N);
        {
            let mut registry = state.terminal_registry.write().await;
            for _ in 0..N {
                let info = make_terminal_info(TerminalStatus::Active);
                let id = info.id;
                ids.push(id);
                registry.register(info);
            }
        }

        // Each task flips its own terminal to Dead.
        let mut set = JoinSet::new();
        for id in ids.clone() {
            let state_c = Arc::clone(&state);
            set.spawn(async move {
                let mut registry = state_c.terminal_registry.write().await;
                registry.update_status(&id, TerminalStatus::Dead);
            });
        }

        while set.join_next().await.is_some() {}

        let registry = state.terminal_registry.read().await;
        for id in &ids {
            let info = registry.get(id).expect("terminal must still exist");
            assert_eq!(
                info.status,
                TerminalStatus::Dead,
                "terminal {id} must be Dead after its owner wrote that status"
            );
        }
    }

    // -----------------------------------------------------------------------
    // S5: concurrent unregister calls on disjoint IDs all succeed
    //
    // N terminals are pre-registered; N tasks each unregister exactly one.
    // After all complete, the registry must be empty. Pins: no double-free
    // or lost-delete when concurrent writers each hold the write lock for
    // their own unregister call.
    // -----------------------------------------------------------------------
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_unregisters_on_disjoint_ids_all_succeed() {
        const N: usize = 16;
        let state = make_state();

        let mut ids = Vec::with_capacity(N);
        {
            let mut registry = state.terminal_registry.write().await;
            for _ in 0..N {
                let info = make_terminal_info(TerminalStatus::Active);
                ids.push(info.id);
                registry.register(info);
            }
        }

        let mut set = JoinSet::new();
        for id in ids {
            let state_c = Arc::clone(&state);
            set.spawn(async move {
                let mut registry = state_c.terminal_registry.write().await;
                let removed = registry.unregister(&id);
                assert!(
                    removed.is_some(),
                    "each terminal must unregister exactly once"
                );
            });
        }

        while set.join_next().await.is_some() {}

        let registry = state.terminal_registry.read().await;
        assert_eq!(
            registry.list().len(),
            0,
            "registry must be empty after all concurrent unregisters"
        );
    }

    // -----------------------------------------------------------------------
    // S6: disconnect buffer ring eviction under concurrent push is bounded
    //
    // N tasks simultaneously push chunks into a shared DisconnectBuffer
    // wrapped in an Arc<RwLock<...>> (matching the ApiState layout).
    // After all pushes complete, the buffer must not exceed its max_bytes
    // capacity (ring eviction must have fired). Pins: the ring-buffer
    // pop_front eviction path is exercised and enforces the bound.
    // -----------------------------------------------------------------------
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn disconnect_buffer_ring_eviction_caps_memory_under_concurrent_push() {
        const CAPACITY: usize = 64; // deliberately tiny to force eviction
        const TASKS: usize = 8;
        const CHUNK: &[u8] = b"ABCDEFGHIJKLMNOP"; // 16 bytes per push

        // Mirror the ApiState layout: shared via Arc<RwLock<HashMap<...>>>.
        let buffers = Arc::new(tokio::sync::RwLock::new({
            let mut m = std::collections::HashMap::new();
            let id = Uuid::new_v4();
            m.insert(id, DisconnectBuffer::new(CAPACITY));
            (id, m)
        }));

        let mut set = JoinSet::new();
        for _ in 0..TASKS {
            let bufs = Arc::clone(&buffers);
            set.spawn(async move {
                for _ in 0..4 {
                    let mut lock = bufs.write().await;
                    let (id, ref mut map) = *lock;
                    if let Some(buf) = map.get_mut(&id) {
                        buf.push(CHUNK);
                    }
                    tokio::task::yield_now().await;
                }
            });
        }

        while set.join_next().await.is_some() {}

        let lock = buffers.read().await;
        let (id, ref map) = *lock;
        let buf = map.get(&id).unwrap();
        assert!(
            buf.data.len() <= CAPACITY,
            "buffer must not exceed capacity ({CAPACITY}); got {}",
            buf.data.len()
        );
    }

    // -----------------------------------------------------------------------
    // S7: disconnect buffer drain races concurrent push — no panic, and
    // drain returns only bytes that were present before the drain call
    //
    // One task drains the buffer while another pushes. The invariant is that
    // the operation completes without panic and the buffer is in a coherent
    // state: its length is bounded by CAPACITY after both tasks finish.
    // -----------------------------------------------------------------------
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn disconnect_buffer_drain_races_push_without_panic() {
        const CAPACITY: usize = 256;
        const CHUNK: &[u8] = b"output-line\n";

        let buf = Arc::new(tokio::sync::Mutex::new(DisconnectBuffer::new(CAPACITY)));

        // Pusher task.
        let buf_push = Arc::clone(&buf);
        let pusher = tokio::spawn(async move {
            for _ in 0..32 {
                buf_push.lock().await.push(CHUNK);
                tokio::task::yield_now().await;
            }
        });

        // Drainer task races the pusher.
        let buf_drain = Arc::clone(&buf);
        let drainer = tokio::spawn(async move {
            let drained = buf_drain.lock().await.drain_all();
            // Whatever was drained must be valid UTF-8 (since we only push
            // ASCII) and must not exceed what the CAPACITY allows.
            assert!(drained.len() <= CAPACITY);
            drained.len()
        });

        pusher.await.expect("pusher task must not panic");
        let drained_len = drainer.await.expect("drainer task must not panic");

        // Post-condition: buffer is still bounded after both tasks finish.
        let final_len = buf.lock().await.data.len();
        assert!(
            final_len <= CAPACITY,
            "buffer must remain bounded (≤{CAPACITY}) after concurrent push+drain; got {final_len}"
        );
        // The drain must have returned something coherent (may be 0 if drain
        // raced ahead of the first push, or up to CAPACITY bytes).
        assert!(drained_len <= CAPACITY);
    }

    // -----------------------------------------------------------------------
    // S8: concurrent disconnect-buffer insertions and lookups on ApiState
    //
    // N tasks each insert a buffer keyed by a unique terminal ID while M
    // reader tasks simultaneously look up existing buffers. No insertion must
    // be lost and no reader must panic. Pins: `Arc<RwLock<HashMap>>` admits
    // concurrent readers and serialises writers correctly.
    // -----------------------------------------------------------------------
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_disconnect_buffer_insert_and_lookup_no_loss() {
        const WRITERS: usize = 16;
        const READERS: usize = 8;

        let state = make_state();
        let inserted_ids: Arc<tokio::sync::Mutex<Vec<Uuid>>> =
            Arc::new(tokio::sync::Mutex::new(Vec::new()));

        // Pre-seed one buffer so readers have something to find immediately.
        let seed_id = Uuid::new_v4();
        {
            let mut buffers = state.disconnect_buffers.write().await;
            buffers.insert(seed_id, DisconnectBuffer::new(DISCONNECT_BUFFER_SIZE));
        }
        inserted_ids.lock().await.push(seed_id);

        let mut set = JoinSet::new();

        // Writer tasks.
        for _ in 0..WRITERS {
            let state_c = Arc::clone(&state);
            let ids_c = Arc::clone(&inserted_ids);
            set.spawn(async move {
                let id = Uuid::new_v4();
                {
                    let mut buffers = state_c.disconnect_buffers.write().await;
                    buffers.insert(id, DisconnectBuffer::new(DISCONNECT_BUFFER_SIZE));
                }
                ids_c.lock().await.push(id);
            });
        }

        // Reader tasks concurrently look up the seed buffer.
        for _ in 0..READERS {
            let state_c = Arc::clone(&state);
            set.spawn(async move {
                let buffers = state_c.disconnect_buffers.read().await;
                // The seed ID must always be present for the reader's lifetime.
                // (Writers only insert new keys, never remove the seed.)
                let _ = buffers.get(&seed_id);
            });
        }

        while set.join_next().await.is_some() {}

        // All writer-inserted IDs must be present.
        let buffers = state.disconnect_buffers.read().await;
        let all_ids = inserted_ids.lock().await;
        for id in all_ids.iter() {
            assert!(
                buffers.contains_key(id),
                "buffer for {id} must survive concurrent insert storm"
            );
        }
        assert_eq!(
            buffers.len(),
            WRITERS + 1,
            "exactly WRITERS+1 buffers must exist (seed + {WRITERS} inserted)"
        );
    }

    // -----------------------------------------------------------------------
    // S9: rename racing with concurrent reads returns consistent title
    //
    // One task renames a terminal while N others read its title concurrently.
    // Each reader must observe either the original title or the new title —
    // never a torn string or a panic. Pins: `rename` holds the write lock for
    // its entire critical section; no reader can observe a partially-written
    // title.
    // -----------------------------------------------------------------------
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn rename_racing_with_concurrent_readers_never_tears() {
        const N: usize = 24;
        let state = make_state();

        let info = make_terminal_info(TerminalStatus::Active);
        let terminal_id = info.id;
        let original_title = info.title.clone();
        let new_title = "renamed-terminal".to_string();

        {
            let mut registry = state.terminal_registry.write().await;
            registry.register(info);
        }

        // Spawn the rename writer.
        let state_w = Arc::clone(&state);
        let new_title_c = new_title.clone();
        let renamer = tokio::spawn(async move {
            let mut registry = state_w.terminal_registry.write().await;
            registry.rename(&terminal_id, new_title_c);
        });

        // Spawn concurrent readers.
        let valid_count = Arc::new(AtomicUsize::new(0));
        let mut set = JoinSet::new();
        for _ in 0..N {
            let state_c = Arc::clone(&state);
            let orig = original_title.clone();
            let new = new_title.clone();
            let valid_c = Arc::clone(&valid_count);
            set.spawn(async move {
                let registry = state_c.terminal_registry.read().await;
                if let Some(t) = registry.get(&terminal_id) {
                    // Title must be one of the two valid values — no torn write.
                    assert!(
                        t.title == orig || t.title == new,
                        "torn title detected: {:?}",
                        t.title
                    );
                    valid_c.fetch_add(1, Ordering::Relaxed);
                }
            });
        }

        renamer.await.expect("rename task must not panic");
        while set.join_next().await.is_some() {}

        // At least some readers must have observed a valid title.
        assert!(
            valid_count.load(Ordering::Relaxed) > 0,
            "at least one reader must have observed the terminal"
        );

        // Final state: the rename must have landed.
        let registry = state.terminal_registry.read().await;
        let final_title = registry.get(&terminal_id).unwrap().title.clone();
        assert_eq!(
            final_title, new_title,
            "rename must be durable after all tasks complete"
        );
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

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let terminals: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(terminals.is_empty());
    }

    /// Test PTY terminal creation.
    ///
    /// NOTE: Requires system permissions to create PTYs (openpty syscall).
    /// Ignored by default as it may fail in sandboxed/restricted environments.
    #[tokio::test]
    #[ignore = "requires PTY creation permissions (openpty syscall)"]
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

    /// Test creating a terminal and listing all terminals.
    ///
    /// NOTE: Requires system permissions to create PTYs (openpty syscall).
    /// Ignored by default as it may fail in sandboxed/restricted environments.
    #[tokio::test]
    #[ignore = "requires PTY creation permissions (openpty syscall)"]
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

    /// Test creating and deleting a terminal.
    ///
    /// NOTE: Requires system permissions to create PTYs (openpty syscall).
    /// Ignored by default as it may fail in sandboxed/restricted environments.
    #[tokio::test]
    #[ignore = "requires PTY creation permissions (openpty syscall)"]
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

// ---------------------------------------------------------------------------
// Reconnect grace-period task unit tests
//
// These tests exercise `spawn_reconnect_grace_task` and `cleanup_disconnected_terminal`
// directly, without a real WebSocket or PTY process.  A fake flume channel
// stands in for the PTY reader so we can simulate both early PTY exit (drop the
// sender before the grace period ends) and normal buffering (keep the sender
// alive and push data through it).
//
// Time-control: `tokio::time::pause()` freezes the Tokio clock.  The tests
// therefore do NOT rely on wall-clock sleeps; they advance time explicitly.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod grace_period_tests {
    use super::*;
    use crate::event_bus::EventBus;
    use crate::terminal::{DisconnectBuffer, TerminalInfo, TerminalStatus, DISCONNECT_BUFFER_SIZE};
    use uuid::Uuid;

    /// Minimal `ApiState` with no PTY pool — sufficient for registry / buffer tests.
    fn make_state() -> Arc<ApiState> {
        Arc::new(ApiState::new(EventBus::new()))
    }

    /// Register a terminal in `Disconnected` state and insert a `DisconnectBuffer`.
    /// Returns the terminal ID.
    async fn setup_disconnected_terminal(state: &Arc<ApiState>) -> Uuid {
        let id = Uuid::new_v4();
        let info = TerminalInfo {
            id,
            agent_id: Uuid::nil(),
            title: "grace-period-test".to_string(),
            status: TerminalStatus::Disconnected {
                since: chrono::Utc::now(),
            },
            cols: 80,
            rows: 24,
            font_size: 14,
            font_family: "monospace".to_string(),
            line_height: 1.0,
            letter_spacing: 0.0,
            profile: "bundled-card".to_string(),
            cursor_style: "block".to_string(),
            cursor_blink: true,
            auto_name: None,
            persistent: false,
        };
        {
            let mut registry = state.terminal_registry.write().await;
            registry.register(info);
        }
        {
            let mut buffers = state.disconnect_buffers.write().await;
            buffers.insert(id, DisconnectBuffer::new(DISCONNECT_BUFFER_SIZE));
        }
        id
    }

    // -----------------------------------------------------------------------
    // T1: PTY exits before grace period — registry and buffer cleaned up immediately
    //
    // This is the regression test for the zombie-terminal leak.
    //
    // When the PTY reader channel closes (sender dropped) before the grace period
    // ends, the task must:
    //   1. Immediately call `cleanup_disconnected_terminal` — NOT wait for the
    //      grace-period timeout.
    //   2. Transition the terminal to `TerminalStatus::Dead`.
    //   3. Remove the `DisconnectBuffer` entry from the registry.
    //
    // On origin/main (before this commit), `spawn_reconnect_grace_task` did not
    // exist as a standalone function; the spawned future was inlined in
    // `handle_terminal_ws`.  This test therefore fails to compile on origin/main,
    // confirming that the early-exit cleanup path was absent.
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn grace_period_task_cleans_up_when_pty_exits_early() {
        tokio::time::pause();

        let state = make_state();
        let terminal_id = setup_disconnected_terminal(&state).await;

        // Create a fake PTY channel.  Drop the sender immediately to simulate
        // the child process having exited before the reconnect window opened.
        let (tx, rx) = flume::bounded::<Vec<u8>>(8);
        drop(tx); // channel closed — recv_async() will return Err immediately

        // Run the grace task to completion (no real timer needed; the closed
        // channel makes it return on the first recv_async call).
        spawn_reconnect_grace_task(Arc::clone(&state), terminal_id, rx).await;

        // Registry must show Dead status.
        {
            let registry = state.terminal_registry.read().await;
            let info = registry
                .get(&terminal_id)
                .expect("terminal must remain in registry (as Dead) after PTY-exit cleanup");
            assert_eq!(
                info.status,
                TerminalStatus::Dead,
                "terminal must be Dead after PTY exits during grace period; got {:?}",
                info.status,
            );
        }

        // Disconnect buffer must have been removed.
        {
            let buffers = state.disconnect_buffers.read().await;
            assert!(
                !buffers.contains_key(&terminal_id),
                "DisconnectBuffer must be removed after PTY exits during grace period",
            );
        }
    }

    // -----------------------------------------------------------------------
    // T2: PTY stays alive — output is buffered and status stays Disconnected
    //
    // Sanity / regression net: when the PTY does NOT exit during the grace
    // period, the task buffers output and keeps the terminal in Disconnected
    // state until the full grace period elapses.
    //
    // We send one chunk through the fake channel, yield so the task can process
    // it, then assert the buffer contents and Disconnected status.  Finally we
    // drop the sender (PTY "exits") and confirm Dead + buffer removed.
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn grace_period_task_buffers_normally_when_pty_stays_alive() {
        tokio::time::pause();

        let state = make_state();
        let terminal_id = setup_disconnected_terminal(&state).await;

        let (tx, rx) = flume::bounded::<Vec<u8>>(8);

        // Spawn the task (don't await — it needs to run concurrently).
        let state_clone = Arc::clone(&state);
        let task = tokio::spawn(spawn_reconnect_grace_task(state_clone, terminal_id, rx));

        // Send some PTY output while the terminal is still in the grace period.
        let chunk = b"hello from pty\n".to_vec();
        tx.send_async(chunk.clone())
            .await
            .expect("send must succeed while grace task is running");

        // Yield so the task can consume the message and push it to the buffer.
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        // Status must still be Disconnected (grace period has not expired yet).
        {
            let registry = state.terminal_registry.read().await;
            let info = registry
                .get(&terminal_id)
                .expect("terminal must be in registry during grace period");
            assert!(
                matches!(info.status, TerminalStatus::Disconnected { .. }),
                "status must be Disconnected while grace period is active; got {:?}",
                info.status,
            );
        }

        // Disconnect buffer must contain the data we sent.
        {
            let buffers = state.disconnect_buffers.read().await;
            let buf = buffers
                .get(&terminal_id)
                .expect("DisconnectBuffer must exist during grace period");
            let buffered: Vec<u8> = buf.data.iter().copied().collect();
            assert!(
                buffered.windows(chunk.len()).any(|w| w == chunk.as_slice()),
                "disconnect buffer must contain the output sent during grace period; \
                 buf = {:?}",
                String::from_utf8_lossy(&buffered),
            );
        }

        // Drop the sender to let the task finish (PTY "exits").
        drop(tx);
        task.await.expect("grace task must not panic");

        // After PTY exit, the terminal should be Dead and buffer gone.
        {
            let registry = state.terminal_registry.read().await;
            let info = registry
                .get(&terminal_id)
                .expect("terminal must remain in registry as Dead");
            assert_eq!(
                info.status,
                TerminalStatus::Dead,
                "terminal must be Dead after PTY exits; got {:?}",
                info.status,
            );
        }
        {
            let buffers = state.disconnect_buffers.read().await;
            assert!(
                !buffers.contains_key(&terminal_id),
                "DisconnectBuffer must be removed after PTY exits",
            );
        }
    }
}
