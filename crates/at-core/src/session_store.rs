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

/// Errors that can occur when persisting or loading session state.
///
/// These errors cover filesystem operations and JSON serialization/deserialization
/// for session files stored in `~/.config/auto-tundra/sessions/`.
#[derive(Debug, thiserror::Error)]
pub enum SessionStoreError {
    /// Failed to read or write session files to disk.
    ///
    /// This typically occurs when:
    /// - The session directory is inaccessible
    /// - Insufficient file permissions
    /// - Disk I/O errors
    /// - Directory creation failed
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Failed to serialize or deserialize session state as JSON.
    ///
    /// This typically occurs when:
    /// - Session file is corrupted or has invalid JSON
    /// - Schema mismatch between stored and current session format
    /// - Non-UTF-8 characters in session file
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
    pub async fn cleanup_old_sessions(
        &self,
        older_than: Duration,
    ) -> Result<usize, SessionStoreError> {
        let cutoff = Utc::now() - older_than;
        let sessions = self.list_sessions().await?;
        let mut removed = 0;
        for session in sessions {
            if session.last_active_at < cutoff {
                match self.delete_session(&session.id).await {
                    Ok(true) => removed += 1,
                    Ok(false) => {}
                    Err(e) => {
                        tracing::warn!(
                            session_id = %session.id,
                            error = %e,
                            "failed to delete expired session; skipping"
                        );
                    }
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

        let removed = store
            .cleanup_old_sessions(Duration::days(30))
            .await
            .unwrap();
        assert_eq!(removed, 1);

        let remaining = store.list_sessions().await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].user_id, "new_user");
    }

    /// Three expired sessions exist; M=3, N=3, fresh=0.
    /// Only expired files are removed, fresh files survive.
    #[tokio::test]
    async fn cleanup_removes_expired_sessions_only() {
        let (store, dir) = temp_store();
        let ttl = Duration::days(30);

        // Create 2 expired sessions
        let mut exp1 = SessionState::new("expired1");
        exp1.last_active_at = Utc::now() - Duration::days(60);
        let mut exp2 = SessionState::new("expired2");
        exp2.last_active_at = Utc::now() - Duration::days(45);
        store.save_session(&exp1).await.unwrap();
        store.save_session(&exp2).await.unwrap();

        // Create 2 fresh sessions
        let fresh1 = SessionState::new("fresh1");
        let fresh2 = SessionState::new("fresh2");
        store.save_session(&fresh1).await.unwrap();
        store.save_session(&fresh2).await.unwrap();

        let removed = store.cleanup_old_sessions(ttl).await.unwrap();
        assert_eq!(removed, 2);

        // Expired files must be gone
        assert!(!dir.path().join(format!("{}.json", exp1.id)).exists());
        assert!(!dir.path().join(format!("{}.json", exp2.id)).exists());

        // Fresh files must remain
        assert!(dir.path().join(format!("{}.json", fresh1.id)).exists());
        assert!(dir.path().join(format!("{}.json", fresh2.id)).exists());
    }

    /// Regression test: cleanup must continue past a file it cannot delete.
    ///
    /// We use `chattr +i` (Linux ext4) to make one expired session file
    /// **immutable**, so that even root cannot remove it. The file is still
    /// **readable** — `list_sessions` will discover it — but `delete_session`
    /// fails with EPERM. The earlier name said "unreadable", which was a
    /// misnomer; renamed to match the actual scenario (undeletable).
    /// The test is skipped at runtime if `chattr` is unavailable or the
    /// filesystem does not support immutable flags (e.g. tmpfs).
    #[tokio::test]
    async fn cleanup_continues_past_undeletable_file() {
        let (store, dir) = temp_store();
        let ttl = Duration::days(30);

        // Create 3 expired sessions.
        let mut exp1 = SessionState::new("exp1");
        exp1.last_active_at = Utc::now() - Duration::days(90);
        let mut exp2 = SessionState::new("exp2");
        exp2.last_active_at = Utc::now() - Duration::days(90);
        let mut exp3 = SessionState::new("exp3");
        exp3.last_active_at = Utc::now() - Duration::days(90);

        store.save_session(&exp1).await.unwrap();
        store.save_session(&exp2).await.unwrap();
        store.save_session(&exp3).await.unwrap();

        // Make exp1's file immutable via `chattr +i` so it cannot be deleted even
        // by root, yet is still readable (list_sessions will see it).
        let blocked_path = dir.path().join(format!("{}.json", exp1.id));
        let chattr_set = std::process::Command::new("chattr")
            .arg("+i")
            .arg(&blocked_path)
            .status();
        let chattr_ok = chattr_set.map(|s| s.success()).unwrap_or(false);
        if !chattr_ok {
            // chattr not available or filesystem doesn't support immutable flag;
            // skip the body — the test still passes (not ignored) but is a no-op.
            return;
        }

        // cleanup must not abort — it should return Ok even though one deletion failed.
        let result = store.cleanup_old_sessions(ttl).await;
        // Restore mutability before any assertions so tempdir cleanup succeeds.
        let _ = std::process::Command::new("chattr")
            .arg("-i")
            .arg(&blocked_path)
            .status();

        let removed = result.expect("cleanup_old_sessions must return Ok");

        // The two deletable files must be gone.
        assert!(!dir.path().join(format!("{}.json", exp2.id)).exists());
        assert!(!dir.path().join(format!("{}.json", exp3.id)).exists());

        // The immutable file must still exist.
        assert!(blocked_path.exists());

        // 2 out of 3 were removed successfully.
        assert_eq!(removed, 2);
    }

    /// Empty session directory. Cleanup must return Ok(()) with 0 removals.
    #[tokio::test]
    async fn cleanup_returns_ok_when_no_sessions_exist() {
        let (store, _dir) = temp_store();
        let result = store.cleanup_old_sessions(Duration::days(30)).await;
        assert!(matches!(result, Ok(0)));
    }

    /// 5 sessions alternating fresh/expired. Only the 2 expired ones are removed.
    #[tokio::test]
    async fn cleanup_with_mixed_fresh_and_expired_preserves_fresh() {
        let (store, dir) = temp_store();
        let ttl = Duration::days(30);

        let mut sessions = Vec::new();
        for i in 0..5_u32 {
            let mut s = SessionState::new(format!("user{i}"));
            if i % 2 == 0 {
                // even indices are expired
                s.last_active_at = Utc::now() - Duration::days(60);
            }
            // odd indices use the default `last_active_at` which is Utc::now() (fresh)
            store.save_session(&s).await.unwrap();
            sessions.push(s);
        }

        let removed = store.cleanup_old_sessions(ttl).await.unwrap();
        assert_eq!(removed, 3); // indices 0, 2, 4 are expired

        for (i, s) in sessions.iter().enumerate() {
            let path = dir.path().join(format!("{}.json", s.id));
            if i % 2 == 0 {
                // expired — must be gone
                assert!(!path.exists(), "expected {path:?} to be deleted");
            } else {
                // fresh — must survive
                assert!(path.exists(), "expected {path:?} to still exist");
            }
        }
    }
}
