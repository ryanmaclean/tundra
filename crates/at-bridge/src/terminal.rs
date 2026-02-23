//! Terminal session management for agent communication.
//!
//! This module provides types and utilities for managing PTY-backed terminal sessions
//! that enable real-time interaction with agents via WebSocket. The registry tracks all
//! terminal instances, handles disconnection/reconnection scenarios, and maintains
//! buffered output during temporary connection loss.
//!
//! ## Key Components
//!
//! * [`TerminalInfo`] — Metadata for a single terminal session (dimensions, status, etc.)
//! * [`TerminalStatus`] — Lifecycle states including `Disconnected` with grace period
//! * [`TerminalEvent`] — WebSocket-friendly messages (output, resize, close, title)
//! * [`DisconnectBuffer`] — Ring buffer that captures PTY output while WebSocket is down
//! * [`TerminalRegistry`] — Thread-safe lookup and status tracking for all terminals
//!
//! ## Disconnection Flow
//!
//! When a WebSocket drops, the terminal transitions to `TerminalStatus::Disconnected`
//! and a [`DisconnectBuffer`] is allocated to capture PTY output. If the client reconnects
//! within [`WS_RECONNECT_GRACE`] (30 seconds), the buffered data is flushed and the
//! terminal resumes normally. Otherwise, the PTY is killed and the terminal moves to
//! `TerminalStatus::Dead`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Terminal Types
// ---------------------------------------------------------------------------

/// Metadata and configuration for a single terminal session.
///
/// Each terminal is backed by a PTY and associated with an agent. The `status`
/// field tracks lifecycle (active, idle, closed, disconnected, dead).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalInfo {
    /// Unique identifier for this terminal session.
    pub id: Uuid,
    /// Agent this terminal belongs to.
    pub agent_id: Uuid,
    /// Human-readable name or title (can be updated dynamically).
    pub title: String,
    /// Current lifecycle state (active, idle, closed, disconnected, dead).
    pub status: TerminalStatus,
    /// Terminal width in columns.
    pub cols: u16,
    /// Terminal height in rows.
    pub rows: u16,
    /// Font size for this terminal (default 14).
    pub font_size: u16,
    /// Cursor style: "block", "underline", or "bar".
    pub cursor_style: String,
    /// Whether to blink the cursor.
    pub cursor_blink: bool,
    /// Auto-generated name from first command output.
    pub auto_name: Option<String>,
    /// Whether this session should persist across restarts.
    pub persistent: bool,
}

/// Lifecycle state of a terminal session.
///
/// Terminals can transition between states as the PTY and WebSocket connection
/// status change. The `Disconnected` state includes a timestamp to enforce the
/// reconnection grace period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalStatus {
    /// Terminal is actively processing commands and connected.
    Active,
    /// Terminal is idle (no recent activity) but still connected.
    Idle,
    /// Terminal has been explicitly closed by the user.
    Closed,
    /// WebSocket disconnected but PTY still running; awaiting reconnect.
    ///
    /// If the WebSocket does not reconnect within [`WS_RECONNECT_GRACE`],
    /// the PTY will be killed and the terminal moved to `Dead`.
    Disconnected {
        /// Timestamp when the disconnection occurred.
        #[serde(with = "chrono::serde::ts_milliseconds")]
        since: DateTime<Utc>,
    },
    /// PTY has been killed after grace period expired (or explicit kill).
    Dead,
}

/// Events emitted by terminals and sent over WebSocket.
///
/// These messages are JSON-serialized with a `type` field discriminator.
/// Clients subscribe to terminal events to receive real-time updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalEvent {
    /// PTY output data (stdout/stderr combined).
    Output {
        /// Terminal that produced this output.
        terminal_id: Uuid,
        /// Raw UTF-8 data from the PTY.
        data: String,
    },
    /// Terminal dimensions have changed (e.g., window resized).
    Resize {
        /// Terminal being resized.
        terminal_id: Uuid,
        /// New width in columns.
        cols: u16,
        /// New height in rows.
        rows: u16,
    },
    /// Terminal has been closed (PTY exited or user-initiated close).
    Close {
        /// Terminal that was closed.
        terminal_id: Uuid,
    },
    /// Terminal title has been updated (e.g., via ANSI escape codes).
    Title {
        /// Terminal whose title changed.
        terminal_id: Uuid,
        /// New title text.
        title: String,
    },
}

// ---------------------------------------------------------------------------
// Disconnect buffer & constants
// ---------------------------------------------------------------------------

/// Grace period for reconnection after a WebSocket disconnect.
///
/// If a client does not reconnect within this duration, the PTY is killed and
/// the terminal transitions to `TerminalStatus::Dead`. Currently set to 30 seconds.
pub const WS_RECONNECT_GRACE: std::time::Duration = std::time::Duration::from_secs(30);

/// Maximum bytes buffered per terminal while disconnected (64 KB).
///
/// This limit prevents unbounded memory growth if a PTY produces output faster
/// than it can be consumed during a prolonged disconnection.
pub const DISCONNECT_BUFFER_SIZE: usize = 65536;

/// Ring buffer that captures PTY output while a terminal is disconnected.
///
/// When a WebSocket drops, we allocate a [`DisconnectBuffer`] and continue reading
/// from the PTY. If the buffer reaches `max_bytes`, the oldest data is discarded
/// (FIFO). Upon reconnection, the entire buffer is flushed to the client.
///
/// # Example
///
/// ```ignore
/// use at_bridge::terminal::{DisconnectBuffer, DISCONNECT_BUFFER_SIZE};
///
/// let mut buf = DisconnectBuffer::new(DISCONNECT_BUFFER_SIZE);
/// buf.push(b"some output");
/// let data = buf.drain_all(); // Vec<u8> ready to send
/// ```
#[derive(Debug)]
pub struct DisconnectBuffer {
    /// Ring buffer of raw bytes from the PTY.
    pub data: VecDeque<u8>,
    /// Maximum capacity; oldest bytes are dropped when exceeded.
    pub max_bytes: usize,
    /// Timestamp when disconnection occurred, used for grace period check.
    pub disconnected_at: DateTime<Utc>,
}

impl DisconnectBuffer {
    /// Create a new buffer with the given capacity and the current time.
    ///
    /// # Parameters
    ///
    /// * `max_bytes` — Maximum number of bytes to buffer before dropping oldest data.
    pub fn new(max_bytes: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(max_bytes),
            max_bytes,
            disconnected_at: Utc::now(),
        }
    }

    /// Append bytes, dropping the oldest if capacity is exceeded.
    ///
    /// This is the core ring-buffer logic: if we're at capacity, remove one byte
    /// from the front before pushing a new byte to the back.
    ///
    /// # Parameters
    ///
    /// * `bytes` — Slice of data to append (typically from a PTY read).
    pub fn push(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if self.data.len() >= self.max_bytes {
                self.data.pop_front();
            }
            self.data.push_back(b);
        }
    }

    /// Drain all buffered bytes into a `Vec<u8>`.
    ///
    /// After this call, the internal buffer is empty and can be reused or dropped.
    /// Typically called when a WebSocket reconnects and we need to flush the backlog.
    pub fn drain_all(&mut self) -> Vec<u8> {
        self.data.drain(..).collect()
    }

    /// Whether the grace period has expired.
    ///
    /// Returns `true` if the elapsed time since `disconnected_at` exceeds
    /// [`WS_RECONNECT_GRACE`]. This signals that the PTY should be killed.
    pub fn grace_expired(&self) -> bool {
        let elapsed = Utc::now()
            .signed_duration_since(self.disconnected_at)
            .to_std()
            .unwrap_or(std::time::Duration::ZERO);
        elapsed >= WS_RECONNECT_GRACE
    }
}

// ---------------------------------------------------------------------------
// Terminal Registry
// ---------------------------------------------------------------------------

/// Central registry for all active terminal sessions.
///
/// The registry provides a thread-safe (when wrapped in `Arc<Mutex<...>>`) lookup
/// table for terminal metadata. It supports querying by ID, listing by status,
/// and updating terminal state (status, title) in place.
///
/// # Example
///
/// ```ignore
/// use at_bridge::terminal::{TerminalRegistry, TerminalInfo, TerminalStatus};
/// use uuid::Uuid;
///
/// let mut registry = TerminalRegistry::new();
/// let info = TerminalInfo {
///     id: Uuid::new_v4(),
///     agent_id: Uuid::new_v4(),
///     title: "My Terminal".into(),
///     status: TerminalStatus::Active,
///     cols: 80,
///     rows: 24,
///     font_size: 14,
///     cursor_style: "block".into(),
///     cursor_blink: true,
///     auto_name: None,
///     persistent: false,
/// };
/// registry.register(info);
/// ```
pub struct TerminalRegistry {
    /// Map from terminal ID to metadata.
    terminals: HashMap<Uuid, TerminalInfo>,
}

impl TerminalRegistry {
    /// Create a new, empty registry.
    pub fn new() -> Self {
        Self {
            terminals: HashMap::new(),
        }
    }

    /// Register a new terminal session.
    ///
    /// # Parameters
    ///
    /// * `info` — Terminal metadata to insert.
    ///
    /// # Returns
    ///
    /// The terminal's ID (copied from `info.id`), for convenience.
    pub fn register(&mut self, info: TerminalInfo) -> Uuid {
        let id = info.id;
        self.terminals.insert(id, info);
        id
    }

    /// Remove a terminal from the registry.
    ///
    /// # Parameters
    ///
    /// * `id` — The terminal ID to remove.
    ///
    /// # Returns
    ///
    /// The removed [`TerminalInfo`], or `None` if not found.
    pub fn unregister(&mut self, id: &Uuid) -> Option<TerminalInfo> {
        self.terminals.remove(id)
    }

    /// Retrieve an immutable reference to a terminal by ID.
    ///
    /// # Parameters
    ///
    /// * `id` — The terminal ID to look up.
    ///
    /// # Returns
    ///
    /// `Some(&TerminalInfo)` if found, otherwise `None`.
    pub fn get(&self, id: &Uuid) -> Option<&TerminalInfo> {
        self.terminals.get(id)
    }

    /// Retrieve a mutable reference to a terminal by ID.
    ///
    /// # Parameters
    ///
    /// * `id` — The terminal ID to look up.
    ///
    /// # Returns
    ///
    /// `Some(&mut TerminalInfo)` if found, otherwise `None`.
    pub fn get_mut(&mut self, id: &Uuid) -> Option<&mut TerminalInfo> {
        self.terminals.get_mut(id)
    }

    /// List all terminals in the registry.
    ///
    /// # Returns
    ///
    /// A vector of immutable references to all registered terminals.
    pub fn list(&self) -> Vec<&TerminalInfo> {
        self.terminals.values().collect()
    }

    /// List only terminals with `status == Active`.
    ///
    /// # Returns
    ///
    /// A vector of immutable references to active terminals.
    pub fn list_active(&self) -> Vec<&TerminalInfo> {
        self.terminals
            .values()
            .filter(|t| t.status == TerminalStatus::Active)
            .collect()
    }

    /// List only terminals with `persistent == true`.
    ///
    /// # Returns
    ///
    /// A vector of immutable references to persistent terminals.
    pub fn list_persistent(&self) -> Vec<&TerminalInfo> {
        self.terminals.values().filter(|t| t.persistent).collect()
    }

    /// Update the status of a terminal in place.
    ///
    /// # Parameters
    ///
    /// * `id` — The terminal ID to update.
    /// * `status` — The new [`TerminalStatus`] to set.
    ///
    /// # Returns
    ///
    /// `true` if the terminal was found and updated, `false` otherwise.
    pub fn update_status(&mut self, id: &Uuid, status: TerminalStatus) -> bool {
        if let Some(t) = self.terminals.get_mut(id) {
            t.status = status;
            true
        } else {
            false
        }
    }

    /// Rename a terminal (update its `title` field).
    ///
    /// # Parameters
    ///
    /// * `id` — The terminal ID to rename.
    /// * `title` — The new title string.
    ///
    /// # Returns
    ///
    /// `true` if the terminal was found and renamed, `false` otherwise.
    pub fn rename(&mut self, id: &Uuid, title: String) -> bool {
        if let Some(t) = self.terminals.get_mut(id) {
            t.title = title;
            true
        } else {
            false
        }
    }
}

impl Default for TerminalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_terminal(status: TerminalStatus) -> TerminalInfo {
        TerminalInfo {
            id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            title: "test terminal".to_string(),
            status,
            cols: 80,
            rows: 24,
            font_size: 14,
            cursor_style: "block".to_string(),
            cursor_blink: true,
            auto_name: None,
            persistent: false,
        }
    }

    #[test]
    fn test_register() {
        let mut reg = TerminalRegistry::new();
        let info = make_terminal(TerminalStatus::Active);
        let id = info.id;
        let returned_id = reg.register(info);
        assert_eq!(returned_id, id);
        assert!(reg.get(&id).is_some());
    }

    #[test]
    fn test_unregister() {
        let mut reg = TerminalRegistry::new();
        let info = make_terminal(TerminalStatus::Active);
        let id = info.id;
        reg.register(info);
        let removed = reg.unregister(&id);
        assert!(removed.is_some());
        assert!(reg.get(&id).is_none());
    }

    #[test]
    fn test_unregister_not_found() {
        let mut reg = TerminalRegistry::new();
        assert!(reg.unregister(&Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_get() {
        let mut reg = TerminalRegistry::new();
        let info = make_terminal(TerminalStatus::Idle);
        let id = info.id;
        reg.register(info);
        let t = reg.get(&id).unwrap();
        assert_eq!(t.title, "test terminal");
        assert_eq!(t.status, TerminalStatus::Idle);
    }

    #[test]
    fn test_list() {
        let mut reg = TerminalRegistry::new();
        reg.register(make_terminal(TerminalStatus::Active));
        reg.register(make_terminal(TerminalStatus::Closed));
        assert_eq!(reg.list().len(), 2);
    }

    #[test]
    fn test_list_active() {
        let mut reg = TerminalRegistry::new();
        reg.register(make_terminal(TerminalStatus::Active));
        reg.register(make_terminal(TerminalStatus::Idle));
        reg.register(make_terminal(TerminalStatus::Closed));
        let active = reg.list_active();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].status, TerminalStatus::Active);
    }

    #[test]
    fn test_update_status() {
        let mut reg = TerminalRegistry::new();
        let info = make_terminal(TerminalStatus::Active);
        let id = info.id;
        reg.register(info);
        assert!(reg.update_status(&id, TerminalStatus::Closed));
        assert_eq!(reg.get(&id).unwrap().status, TerminalStatus::Closed);
    }

    #[test]
    fn test_update_status_not_found() {
        let mut reg = TerminalRegistry::new();
        assert!(!reg.update_status(&Uuid::new_v4(), TerminalStatus::Active));
    }

    #[test]
    fn test_default_font_settings() {
        let info = make_terminal(TerminalStatus::Active);
        assert_eq!(info.font_size, 14);
        assert_eq!(info.cursor_style, "block");
        assert!(info.cursor_blink);
        assert!(info.auto_name.is_none());
        assert!(!info.persistent);
    }

    #[test]
    fn test_persistent_flag() {
        let mut reg = TerminalRegistry::new();
        let mut info = make_terminal(TerminalStatus::Active);
        let id = info.id;
        info.persistent = true;
        reg.register(info);

        let persistent = reg.list_persistent();
        assert_eq!(persistent.len(), 1);
        assert_eq!(persistent[0].id, id);

        // Non-persistent terminal should not appear.
        reg.register(make_terminal(TerminalStatus::Active));
        assert_eq!(reg.list_persistent().len(), 1);
    }

    #[test]
    fn test_auto_naming() {
        let mut reg = TerminalRegistry::new();
        let info = make_terminal(TerminalStatus::Active);
        let id = info.id;
        reg.register(info);

        let terminal = reg.get_mut(&id).unwrap();
        assert!(terminal.auto_name.is_none());

        terminal.auto_name = Some("cargo build".to_string());
        assert_eq!(
            reg.get(&id).unwrap().auto_name.as_deref(),
            Some("cargo build")
        );
    }

    #[test]
    fn test_get_mut() {
        let mut reg = TerminalRegistry::new();
        let info = make_terminal(TerminalStatus::Active);
        let id = info.id;
        reg.register(info);

        let terminal = reg.get_mut(&id).unwrap();
        terminal.font_size = 18;
        terminal.cursor_style = "underline".to_string();
        terminal.cursor_blink = false;

        let t = reg.get(&id).unwrap();
        assert_eq!(t.font_size, 18);
        assert_eq!(t.cursor_style, "underline");
        assert!(!t.cursor_blink);
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut info = make_terminal(TerminalStatus::Active);
        info.font_size = 16;
        info.cursor_style = "bar".to_string();
        info.cursor_blink = false;
        info.auto_name = Some("npm start".to_string());
        info.persistent = true;

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: TerminalInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, info.id);
        assert_eq!(deserialized.font_size, 16);
        assert_eq!(deserialized.cursor_style, "bar");
        assert!(!deserialized.cursor_blink);
        assert_eq!(deserialized.auto_name, Some("npm start".to_string()));
        assert!(deserialized.persistent);
        assert_eq!(deserialized.cols, 80);
        assert_eq!(deserialized.rows, 24);
    }

    #[test]
    fn test_disconnect_buffer_push_and_drain() {
        let mut buf = DisconnectBuffer::new(16);
        buf.push(b"hello");
        buf.push(b" world");
        let out = buf.drain_all();
        assert_eq!(out, b"hello world");
    }

    #[test]
    fn test_disconnect_buffer_bounded() {
        let mut buf = DisconnectBuffer::new(8);
        buf.push(b"abcdefghij"); // 10 bytes into 8-byte buffer
        let out = buf.drain_all();
        // Oldest bytes dropped: "ab" gone, remaining "cdefghij"
        assert_eq!(out, b"cdefghij");
    }

    #[test]
    fn test_disconnect_buffer_drain_empties() {
        let mut buf = DisconnectBuffer::new(64);
        buf.push(b"data");
        let _ = buf.drain_all();
        assert!(buf.data.is_empty());
    }

    #[test]
    fn test_disconnected_status_variant() {
        let status = TerminalStatus::Disconnected { since: Utc::now() };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("disconnected"));
    }

    #[test]
    fn test_dead_status_variant() {
        let info = make_terminal(TerminalStatus::Dead);
        assert_eq!(info.status, TerminalStatus::Dead);
    }
}
