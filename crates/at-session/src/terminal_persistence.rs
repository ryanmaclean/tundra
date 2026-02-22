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
