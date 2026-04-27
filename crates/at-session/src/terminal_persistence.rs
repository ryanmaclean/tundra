use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Persisted terminal session metadata (saved to disk, restored on restart).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedTerminal {
    pub id: String,
    pub title: String,
    pub shell: String,
    pub working_dir: String,
    pub env_vars: Vec<(String, String)>,
    pub created_at: String,
    pub scroll_buffer_path: Option<String>,
}

/// Store for terminal session persistence. Saves/loads from a JSON file.
pub struct TerminalPersistence {
    path: PathBuf,
}

impl TerminalPersistence {
    pub fn new(data_dir: &std::path::Path) -> Self {
        Self {
            path: data_dir.join("terminal_sessions.json"),
        }
    }

    pub fn save(&self, sessions: &[PersistedTerminal]) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(sessions)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    pub fn load(&self) -> anyhow::Result<Vec<PersistedTerminal>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let json = std::fs::read_to_string(&self.path)?;
        let sessions: Vec<PersistedTerminal> = serde_json::from_str(&json)?;
        Ok(sessions)
    }

    pub fn clear(&self) -> anyhow::Result<()> {
        if self.path.exists() {
            std::fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    /// Produce a unique scratch directory under the system temp dir without
    /// requiring `tempfile` as a dev-dependency. The directory is created on
    /// demand and cleaned up by the returned [`ScratchDir`] guard on drop.
    struct ScratchDir(PathBuf);

    impl ScratchDir {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .join(format!("at-session-test-{}", Uuid::new_v4()));
            std::fs::create_dir_all(&path).expect("create scratch dir");
            ScratchDir(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn sample_terminal(id: &str) -> PersistedTerminal {
        PersistedTerminal {
            id: id.into(),
            title: format!("Title {id}"),
            shell: "/bin/bash".into(),
            working_dir: "/home/user".into(),
            env_vars: vec![("KEY".into(), "value".into())],
            created_at: "2026-04-27T00:00:00Z".into(),
            scroll_buffer_path: Some(format!("/var/buffers/{id}.log")),
        }
    }

    // -- Path layout --------------------------------------------------------

    #[test]
    fn new_appends_terminal_sessions_json_filename() {
        let dir = ScratchDir::new();
        let store = TerminalPersistence::new(dir.path());
        // We can't read the private `path` field directly; instead, save and
        // verify the expected file appears.
        store.save(&[]).expect("save empty");
        let expected = dir.path().join("terminal_sessions.json");
        assert!(
            expected.exists(),
            "expected file at {expected:?} after save"
        );
    }

    // -- load() on missing file -------------------------------------------

    #[test]
    fn load_returns_empty_when_file_missing() {
        let dir = ScratchDir::new();
        let store = TerminalPersistence::new(dir.path());
        let loaded = store.load().expect("load missing should not error");
        assert!(loaded.is_empty());
    }

    // -- save / load round-trip -------------------------------------------

    #[test]
    fn save_and_load_round_trip_preserves_data() {
        let dir = ScratchDir::new();
        let store = TerminalPersistence::new(dir.path());
        let original = vec![sample_terminal("a"), sample_terminal("b")];
        store.save(&original).expect("save");

        let loaded = store.load().expect("load");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id, "a");
        assert_eq!(loaded[1].id, "b");
        assert_eq!(loaded[0].title, "Title a");
        assert_eq!(loaded[0].shell, "/bin/bash");
        assert_eq!(loaded[0].working_dir, "/home/user");
        assert_eq!(loaded[0].env_vars, vec![("KEY".into(), "value".into())]);
        assert_eq!(loaded[0].created_at, "2026-04-27T00:00:00Z");
        assert_eq!(
            loaded[0].scroll_buffer_path.as_deref(),
            Some("/var/buffers/a.log")
        );
    }

    #[test]
    fn save_overwrites_existing_file() {
        let dir = ScratchDir::new();
        let store = TerminalPersistence::new(dir.path());
        store.save(&[sample_terminal("first")]).expect("save 1");
        store.save(&[sample_terminal("second")]).expect("save 2");
        let loaded = store.load().expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "second");
    }

    #[test]
    fn save_creates_missing_parent_directories() {
        let dir = ScratchDir::new();
        let nested = dir.path().join("nested").join("deeper");
        // `nested` does not exist yet — save() should mkdir -p it.
        let store = TerminalPersistence::new(&nested);
        store.save(&[sample_terminal("x")]).expect("save");
        assert!(nested.join("terminal_sessions.json").exists());
    }

    #[test]
    fn save_empty_slice_writes_empty_array() {
        let dir = ScratchDir::new();
        let store = TerminalPersistence::new(dir.path());
        store.save(&[]).expect("save empty");
        let loaded = store.load().expect("load");
        assert!(loaded.is_empty());
    }

    // -- clear() -----------------------------------------------------------

    #[test]
    fn clear_removes_existing_file() {
        let dir = ScratchDir::new();
        let store = TerminalPersistence::new(dir.path());
        store.save(&[sample_terminal("x")]).expect("save");
        let path = dir.path().join("terminal_sessions.json");
        assert!(path.exists());
        store.clear().expect("clear");
        assert!(!path.exists());
    }

    #[test]
    fn clear_is_idempotent_when_file_missing() {
        let dir = ScratchDir::new();
        let store = TerminalPersistence::new(dir.path());
        // No save first — the file does not yet exist.
        store.clear().expect("clear with no file should not error");
        store.clear().expect("clear again should still not error");
    }

    // -- serde shape -------------------------------------------------------

    #[test]
    fn persisted_terminal_serializes_to_expected_json_keys() {
        let term = sample_terminal("z");
        let json = serde_json::to_value(&term).expect("serialize");
        let obj = json.as_object().expect("json object");
        for key in [
            "id",
            "title",
            "shell",
            "working_dir",
            "env_vars",
            "created_at",
            "scroll_buffer_path",
        ] {
            assert!(obj.contains_key(key), "missing key '{key}' in {obj:?}");
        }
    }

    #[test]
    fn persisted_terminal_deserializes_with_null_scroll_buffer_path() {
        let json = r#"{
            "id": "t1",
            "title": "Title",
            "shell": "/bin/sh",
            "working_dir": "/tmp",
            "env_vars": [],
            "created_at": "2026-04-27T00:00:00Z",
            "scroll_buffer_path": null
        }"#;
        let term: PersistedTerminal = serde_json::from_str(json).expect("deserialize");
        assert_eq!(term.id, "t1");
        assert!(term.scroll_buffer_path.is_none());
        assert!(term.env_vars.is_empty());
    }

    #[test]
    fn load_invalid_json_returns_error() {
        let dir = ScratchDir::new();
        let path = dir.path().join("terminal_sessions.json");
        std::fs::write(&path, b"not json at all").expect("write bad json");
        let store = TerminalPersistence::new(dir.path());
        let result = store.load();
        assert!(result.is_err(), "expected load to fail on bad JSON");
    }
}
