use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalInfo {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub title: String,
    pub status: TerminalStatus,
    pub cols: u16,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalStatus {
    Active,
    Idle,
    Closed,
    /// WebSocket disconnected but PTY still running; awaiting reconnect.
    Disconnected {
        #[serde(with = "chrono::serde::ts_milliseconds")]
        since: DateTime<Utc>,
    },
    /// PTY has been killed after grace period expired (or explicit kill).
    Dead,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalEvent {
    Output { terminal_id: Uuid, data: String },
    Resize { terminal_id: Uuid, cols: u16, rows: u16 },
    Close { terminal_id: Uuid },
    Title { terminal_id: Uuid, title: String },
}

// ---------------------------------------------------------------------------
// Disconnect buffer & constants
// ---------------------------------------------------------------------------

/// Grace period for reconnection after a WebSocket disconnect.
pub const WS_RECONNECT_GRACE: std::time::Duration = std::time::Duration::from_secs(30);

/// Maximum bytes buffered per terminal while disconnected (64 KB).
pub const DISCONNECT_BUFFER_SIZE: usize = 65536;

/// Ring buffer that captures PTY output while a terminal is disconnected.
/// When `max_bytes` is reached the oldest bytes are dropped.
#[derive(Debug)]
pub struct DisconnectBuffer {
    pub data: VecDeque<u8>,
    pub max_bytes: usize,
    pub disconnected_at: DateTime<Utc>,
}

impl DisconnectBuffer {
    /// Create a new buffer with the given capacity and the current time.
    pub fn new(max_bytes: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(max_bytes),
            max_bytes,
            disconnected_at: Utc::now(),
        }
    }

    /// Append bytes, dropping the oldest if capacity is exceeded.
    pub fn push(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if self.data.len() >= self.max_bytes {
                self.data.pop_front();
            }
            self.data.push_back(b);
        }
    }

    /// Drain all buffered bytes into a `Vec<u8>`.
    pub fn drain_all(&mut self) -> Vec<u8> {
        self.data.drain(..).collect()
    }

    /// Whether the grace period has expired.
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

pub struct TerminalRegistry {
    terminals: HashMap<Uuid, TerminalInfo>,
}

impl TerminalRegistry {
    pub fn new() -> Self {
        Self {
            terminals: HashMap::new(),
        }
    }

    pub fn register(&mut self, info: TerminalInfo) -> Uuid {
        let id = info.id;
        self.terminals.insert(id, info);
        id
    }

    pub fn unregister(&mut self, id: &Uuid) -> Option<TerminalInfo> {
        self.terminals.remove(id)
    }

    pub fn get(&self, id: &Uuid) -> Option<&TerminalInfo> {
        self.terminals.get(id)
    }

    pub fn get_mut(&mut self, id: &Uuid) -> Option<&mut TerminalInfo> {
        self.terminals.get_mut(id)
    }

    pub fn list(&self) -> Vec<&TerminalInfo> {
        self.terminals.values().collect()
    }

    pub fn list_active(&self) -> Vec<&TerminalInfo> {
        self.terminals
            .values()
            .filter(|t| t.status == TerminalStatus::Active)
            .collect()
    }

    pub fn list_persistent(&self) -> Vec<&TerminalInfo> {
        self.terminals
            .values()
            .filter(|t| t.persistent)
            .collect()
    }

    pub fn update_status(&mut self, id: &Uuid, status: TerminalStatus) -> bool {
        if let Some(t) = self.terminals.get_mut(id) {
            t.status = status;
            true
        } else {
            false
        }
    }

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
        assert_eq!(reg.get(&id).unwrap().auto_name.as_deref(), Some("cargo build"));
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
