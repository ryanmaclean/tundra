use chrono::{DateTime, Duration, Utc};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use tokio::sync::Mutex;
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

/// File-system-backed session persistence with in-memory LRU cache.
///
/// Sessions are stored as individual JSON files under a configurable directory
/// (defaults to `~/.config/auto-tundra/sessions/`). An in-memory LRU cache
/// improves read performance by avoiding filesystem I/O for recently accessed sessions.
pub struct SessionStore {
    base_dir: PathBuf,
    cache: Mutex<LruCache<Uuid, SessionState>>,
}

impl SessionStore {
    /// Create a store with the default directory (`~/.config/auto-tundra/sessions/`).
    pub fn default_path() -> Self {
        let base = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join("auto-tundra")
            .join("sessions");
        let capacity = NonZeroUsize::new(100).expect("100 is non-zero");
        Self {
            base_dir: base,
            cache: Mutex::new(LruCache::new(capacity)),
        }
    }

    /// Create a store backed by a custom directory (useful for testing).
    pub fn new(base_dir: PathBuf) -> Self {
        let capacity = NonZeroUsize::new(100).expect("100 is non-zero");
        Self {
            base_dir,
            cache: Mutex::new(LruCache::new(capacity)),
        }
    }

    /// Ensure the base directory exists.
    async fn ensure_dir(&self) -> Result<(), SessionStoreError> {
        tokio::fs::create_dir_all(&self.base_dir).await?;
        Ok(())
    }

    /// Path for a given session ID.
    fn session_path(&self, id: &Uuid) -> PathBuf {
        self.base_dir.join(format!("{}.json", id))
    }

    /// Save a session to disk and update the cache.
    pub async fn save_session(&self, state: &SessionState) -> Result<(), SessionStoreError> {
        self.ensure_dir().await?;
        let path = self.session_path(&state.id);
        let json = serde_json::to_string_pretty(state)?;
        tokio::fs::write(path, json).await?;

        // Update cache with latest state
        let mut cache = self.cache.lock().await;
        cache.put(state.id, state.clone());

        Ok(())
    }

    /// Load a session by ID. Returns `None` if not found.
    /// Checks the in-memory cache first before reading from filesystem.
    pub async fn load_session(&self, id: &Uuid) -> Result<Option<SessionState>, SessionStoreError> {
        // Check cache first
        {
            let mut cache = self.cache.lock().await;
            if let Some(state) = cache.get(id) {
                return Ok(Some(state.clone()));
            }
        }

        // Cache miss - load from filesystem
        let path = self.session_path(id);
        match tokio::fs::try_exists(&path).await {
            Ok(false) => return Ok(None),
            Err(e) => return Err(SessionStoreError::Io(e)),
            Ok(true) => {}
        }
        let data = tokio::fs::read_to_string(path).await?;
        let state: SessionState = serde_json::from_str(&data)?;

        // Populate cache for future reads
        {
            let mut cache = self.cache.lock().await;
            cache.put(*id, state.clone());
        }

        Ok(Some(state))
    }

    /// List all saved sessions, sorted by last active time (most recent first).
    /// Populates the cache with sessions as they are read from filesystem.
    pub async fn list_sessions(&self) -> Result<Vec<SessionState>, SessionStoreError> {
        self.ensure_dir().await?;
        let mut sessions = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&self.base_dir).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                match tokio::fs::read_to_string(&path).await {
                    Ok(data) => {
                        if let Ok(state) = serde_json::from_str::<SessionState>(&data) {
                            sessions.push(state.clone());

                            // Populate cache for future reads
                            let mut cache = self.cache.lock().await;
                            cache.put(state.id, state);
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
    /// Also removes the session from cache if present.
    pub async fn delete_session(&self, id: &Uuid) -> Result<bool, SessionStoreError> {
        let path = self.session_path(id);
        let result = match tokio::fs::try_exists(&path).await {
            Ok(true) => {
                tokio::fs::remove_file(path).await?;
                true
            }
            Ok(false) => false,
            Err(e) => return Err(SessionStoreError::Io(e)),
        };

        // Remove from cache regardless of filesystem result
        let mut cache = self.cache.lock().await;
        cache.pop(id);

        Ok(result)
    }

    /// Delete sessions whose `last_active_at` is older than `older_than`
    /// duration from now. Returns the number of sessions removed.
    pub async fn cleanup_old_sessions(&self, older_than: Duration) -> Result<usize, SessionStoreError> {
        let cutoff = Utc::now() - older_than;
        let sessions = self.list_sessions().await?;
        let mut removed = 0;
        for session in sessions {
            if session.last_active_at < cutoff {
                if self.delete_session(&session.id).await? {
                    removed += 1;
                }
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

    #[tokio::test]
    async fn test_save_and_load_roundtrip() {
        let (store, _dir) = temp_store();
        let mut state = SessionState::new("alice");
        state.active_page = "tasks".to_string();
        state.sidebar_collapsed = true;
        state.terminal_layout = TerminalLayout::SplitHorizontal;
        state.filters.insert("status".into(), "active".into());

        store.save_session(&state).await.unwrap();
        let loaded = store.load_session(&state.id).await.unwrap().unwrap();

        assert_eq!(loaded.id, state.id);
        assert_eq!(loaded.user_id, "alice");
        assert_eq!(loaded.active_page, "tasks");
        assert!(loaded.sidebar_collapsed);
        assert_eq!(loaded.terminal_layout, TerminalLayout::SplitHorizontal);
        assert_eq!(loaded.filters.get("status").unwrap(), "active");
    }

    #[tokio::test]
    async fn test_load_nonexistent() {
        let (store, _dir) = temp_store();
        let result = store.load_session(&Uuid::new_v4()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let (store, _dir) = temp_store();

        let s1 = SessionState::new("alice");
        let s2 = SessionState::new("bob");
        store.save_session(&s1).await.unwrap();
        store.save_session(&s2).await.unwrap();

        let list = store.list_sessions().await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_session() {
        let (store, _dir) = temp_store();
        let state = SessionState::new("alice");
        store.save_session(&state).await.unwrap();

        assert!(store.delete_session(&state.id).await.unwrap());
        assert!(!store.delete_session(&state.id).await.unwrap()); // already gone
        assert!(store.load_session(&state.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_cleanup_old_sessions() {
        let (store, _dir) = temp_store();

        // Create an old session
        let mut old = SessionState::new("old_user");
        old.last_active_at = Utc::now() - Duration::days(90);
        store.save_session(&old).await.unwrap();

        // Create a recent session
        let recent = SessionState::new("new_user");
        store.save_session(&recent).await.unwrap();

        let removed = store.cleanup_old_sessions(Duration::days(30)).await.unwrap();
        assert_eq!(removed, 1);

        let remaining = store.list_sessions().await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].user_id, "new_user");
    }
}
