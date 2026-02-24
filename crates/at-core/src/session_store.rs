use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The layout of terminal panels in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TerminalLayout {
    #[default]
    Single,
    SplitHorizontal,
    SplitVertical,
    Grid2x2,
}

/// Persisted UI session state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub id: Uuid,
    pub user_id: String,
    pub active_page: String,
    pub sidebar_collapsed: bool,
    pub selected_bead_id: Option<Uuid>,
    pub terminal_layout: TerminalLayout,
    pub filters: HashMap<String, String>,
    pub last_active_at: DateTime<Utc>,
}

impl SessionState {
    /// Create a new session state with sensible defaults.
    pub fn new(user_id: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id: user_id.into(),
            active_page: "dashboard".to_string(),
            sidebar_collapsed: false,
            selected_bead_id: None,
            terminal_layout: TerminalLayout::default(),
            filters: HashMap::new(),
            last_active_at: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum SessionStoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

// ---------------------------------------------------------------------------
// SessionStore
// ---------------------------------------------------------------------------

/// File-system-backed session persistence.
///
/// Sessions are stored as individual JSON files under a configurable directory
/// (defaults to `~/.config/auto-tundra/sessions/`).
pub struct SessionStore {
    base_dir: PathBuf,
}

impl SessionStore {
    /// Create a store with the default directory (`~/.config/auto-tundra/sessions/`).
    pub fn default_path() -> Self {
        let base = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join("auto-tundra")
            .join("sessions");
        Self { base_dir: base }
    }

    /// Create a store backed by a custom directory (useful for testing).
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Ensure the base directory exists.
    fn ensure_dir(&self) -> Result<(), SessionStoreError> {
        std::fs::create_dir_all(&self.base_dir)?;
        Ok(())
    }

    /// Path for a given session ID.
    fn session_path(&self, id: &Uuid) -> PathBuf {
        self.base_dir.join(format!("{}.json", id))
    }

    /// Save a session to disk.
    pub fn save_session(&self, state: &SessionState) -> Result<(), SessionStoreError> {
        self.ensure_dir()?;
        let path = self.session_path(&state.id);
        let json = serde_json::to_string_pretty(state)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load a session by ID. Returns `None` if not found.
    pub fn load_session(&self, id: &Uuid) -> Result<Option<SessionState>, SessionStoreError> {
        let path = self.session_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read_to_string(path)?;
        let state: SessionState = serde_json::from_str(&data)?;
        Ok(Some(state))
    }

    /// List all saved sessions, sorted by last active time (most recent first).
    pub fn list_sessions(&self) -> Result<Vec<SessionState>, SessionStoreError> {
        self.ensure_dir()?;
        let mut sessions = Vec::new();
        for entry in std::fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                match std::fs::read_to_string(&path) {
                    Ok(data) => {
                        if let Ok(state) = serde_json::from_str::<SessionState>(&data) {
                            sessions.push(state);
                        }
                    }
                    Err(_) => continue,
                }
            }
        }
        sessions.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
        Ok(sessions)
    }

    /// Delete a session by ID. Returns `true` if the file was removed.
    pub fn delete_session(&self, id: &Uuid) -> Result<bool, SessionStoreError> {
        let path = self.session_path(id);
        if path.exists() {
            std::fs::remove_file(path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Delete sessions whose `last_active_at` is older than `older_than`
    /// duration from now. Returns the number of sessions removed.
    ///
    /// Uses a lightweight partial deserialization to extract only the
    /// `id` and `last_active_at` fields, avoiding full `SessionState`
    /// parsing for sessions that will just be deleted.
    pub fn cleanup_old_sessions(&self, older_than: Duration) -> Result<usize, SessionStoreError> {
        self.ensure_dir()?;
        let cutoff = Utc::now() - older_than;
        let mut removed = 0;

        // Lightweight struct for partial deserialization — only the fields we need.
        #[derive(Deserialize)]
        struct SessionMeta {
            #[allow(dead_code)]
            id: Uuid,
            last_active_at: DateTime<Utc>,
        }

        for entry in std::fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let data = match std::fs::read_to_string(&path) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let meta: SessionMeta = match serde_json::from_str(&data) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.last_active_at < cutoff {
                std::fs::remove_file(&path)?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (SessionStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let store = SessionStore::new(dir.path().to_path_buf());
        (store, dir)
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let (store, _dir) = temp_store();
        let mut state = SessionState::new("alice");
        state.active_page = "tasks".to_string();
        state.sidebar_collapsed = true;
        state.terminal_layout = TerminalLayout::SplitHorizontal;
        state.filters.insert("status".into(), "active".into());

        store.save_session(&state).unwrap();
        let loaded = store.load_session(&state.id).unwrap().unwrap();

        assert_eq!(loaded.id, state.id);
        assert_eq!(loaded.user_id, "alice");
        assert_eq!(loaded.active_page, "tasks");
        assert!(loaded.sidebar_collapsed);
        assert_eq!(loaded.terminal_layout, TerminalLayout::SplitHorizontal);
        assert_eq!(loaded.filters.get("status").unwrap(), "active");
    }

    #[test]
    fn test_load_nonexistent() {
        let (store, _dir) = temp_store();
        let result = store.load_session(&Uuid::new_v4()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_sessions() {
        let (store, _dir) = temp_store();

        let s1 = SessionState::new("alice");
        let s2 = SessionState::new("bob");
        store.save_session(&s1).unwrap();
        store.save_session(&s2).unwrap();

        let list = store.list_sessions().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_delete_session() {
        let (store, _dir) = temp_store();
        let state = SessionState::new("alice");
        store.save_session(&state).unwrap();

        assert!(store.delete_session(&state.id).unwrap());
        assert!(!store.delete_session(&state.id).unwrap()); // already gone
        assert!(store.load_session(&state.id).unwrap().is_none());
    }

    #[test]
    fn test_cleanup_old_sessions() {
        let (store, _dir) = temp_store();

        // Create an old session
        let mut old = SessionState::new("old_user");
        old.last_active_at = Utc::now() - Duration::days(90);
        store.save_session(&old).unwrap();

        // Create a recent session
        let recent = SessionState::new("new_user");
        store.save_session(&recent).unwrap();

        let removed = store.cleanup_old_sessions(Duration::days(30)).unwrap();
        assert_eq!(removed, 1);

        let remaining = store.list_sessions().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].user_id, "new_user");
    }
}
